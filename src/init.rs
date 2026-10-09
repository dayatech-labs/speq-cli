//! `speq list` and `speq init`.
use crate::api::ProjectSummary;
use crate::env::Env;
use crate::error::{Error, Result};
use crate::paths::is_valid_local_dir;
use crate::session::Session;
use crate::workspace::{Config, SPEQ_DIR, SpecSource, Workspace, ensure_gitignore};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ListReport {
    pub user_id: String,
    pub expires_at: String,
    pub projects: Vec<ProjectSummary>,
}

/// From the snapshot when it is still valid; refreshed in the foreground otherwise.
pub async fn list(env: &Env) -> Result<ListReport> {
    let snapshot = Session::new(env).snapshot().await?;
    Ok(ListReport {
        user_id: snapshot.user_id,
        expires_at: snapshot
            .expires_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        projects: snapshot.projects,
    })
}

/// Matches `owner/repo`, the bare repo name, or the project's display name (all case-insensitive).
/// It must identify exactly one project the user can use that is bound to a repository.
pub fn resolve_repo<'a>(
    projects: &'a [ProjectSummary],
    wanted: &str,
) -> Result<&'a ProjectSummary> {
    let matches: Vec<(&ProjectSummary, &str)> = projects
        .iter()
        .filter_map(|p| p.repository_full_name.as_deref().map(|r| (p, r)))
        .filter(|(p, full)| {
            let short = full.rsplit('/').next().unwrap_or(full);
            [*full, short, p.display_name.as_str()]
                .iter()
                .any(|name| name.eq_ignore_ascii_case(wanted))
        })
        .collect();
    match matches.as_slice() {
        [] => Err(Error::invalid(format!(
            "{wanted:?} is not in your repository list; see `speq list`"
        ))),
        [(project, _)] => Ok(project),
        many => Err(Error::invalid(format!(
            "{wanted:?} is ambiguous; use the full name: {}",
            many.iter()
                .map(|(_, full)| *full)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[derive(Debug, Serialize)]
pub struct InitReport {
    pub project_id: String,
    pub repository: String,
    pub branch: String,
    pub local_dir: String,
    pub reconfigured: bool,
    /// Lock, manifest, and baselines were cleared because the source changed.
    pub sync_state_reset: bool,
}

pub struct InitOptions<'a> {
    pub repo: &'a str,
    pub local_dir: &'a str,
    pub reconfigure: bool,
}

pub async fn init(env: &Env, root: &Path, options: InitOptions<'_>) -> Result<InitReport> {
    let existing = root.join(SPEQ_DIR).join("config.toml").exists();
    if existing && !options.reconfigure {
        return Err(Error::invalid(
            "this directory is already initialized; run `speq init <repo> --reconfigure` to change it",
        ));
    }
    if !is_valid_local_dir(options.local_dir) {
        return Err(Error::invalid(
            "--local-dir must be a relative folder inside the repository, without `..` or hidden segments",
        ));
    }

    // The list comes from the API (via the snapshot); it never lets a user pick an unlisted repository.
    let snapshot = Session::new(env).snapshot().await?;
    let project = resolve_repo(&snapshot.projects, options.repo)?;
    let config = Config {
        spec_source: SpecSource {
            project_id: project.project_id.clone(),
            repository: project.repository_full_name.clone().unwrap_or_default(),
            branch: project.branch.clone(),
            local_dir: options.local_dir.to_string(),
        },
    };

    let mut reset = false;
    if existing {
        let previous = Workspace::open(root)?;
        let moved = previous.config.spec_source.project_id != config.spec_source.project_id
            || previous.config.spec_source.branch != config.spec_source.branch
            || previous.config.spec_source.local_dir != config.spec_source.local_dir;
        if moved {
            previous.reset_sync_state()?;
            reset = true;
        }
    }
    let ws = Workspace::create(root, &config)?;
    ensure_gitignore(&ws.root, &config.spec_source.local_dir)?;
    Ok(InitReport {
        project_id: config.spec_source.project_id,
        repository: config.spec_source.repository,
        branch: config.spec_source.branch,
        local_dir: config.spec_source.local_dir,
        reconfigured: existing,
        sync_state_reset: reset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Capabilities, RuleSet};

    fn project(id: &str, display: &str, repo: Option<&str>) -> ProjectSummary {
        let rules = RuleSet {
            allow: vec![],
            deny: vec![],
        };
        ProjectSummary {
            project_id: id.into(),
            display_name: display.into(),
            repository_full_name: repo.map(Into::into),
            branch: "main".into(),
            roles: vec![],
            capabilities: Capabilities {
                create: rules.clone(),
                update: rules,
                can_delete: false,
                can_restore: false,
                can_manage: false,
            },
        }
    }

    #[test]
    fn short_names_must_be_unique() {
        let projects = vec![
            project("1", "Payment", Some("acme/payment-specs")),
            project("2", "Payment", Some("other/payment-specs")),
            project("3", "Cart", Some("acme/cart-specs")),
            project("4", "Unbound", None),
        ];
        assert_eq!(
            resolve_repo(&projects, "cart-specs").unwrap().project_id,
            "3"
        );
        assert_eq!(
            resolve_repo(&projects, "Other/Payment-Specs")
                .unwrap()
                .project_id,
            "2"
        );
        let ambiguous = resolve_repo(&projects, "payment-specs")
            .unwrap_err()
            .to_string();
        assert!(
            ambiguous.contains("acme/payment-specs") && ambiguous.contains("other/payment-specs")
        );
        assert!(resolve_repo(&projects, "missing").is_err());
    }

    #[test]
    fn display_name_resolves_when_unique() {
        let projects = vec![
            project("1", "Engauge", Some("acme/engauge-specs")),
            project("2", "Payment", Some("acme/payment-specs")),
            project("3", "Payment", Some("other/payment-specs")),
            project("4", "Orphan", None),
        ];
        assert_eq!(resolve_repo(&projects, "engauge").unwrap().project_id, "1");
        let ambiguous = resolve_repo(&projects, "Payment").unwrap_err().to_string();
        assert!(
            ambiguous.contains("acme/payment-specs") && ambiguous.contains("other/payment-specs")
        );
        // A project without a repository cannot be initialized, even by display name.
        assert!(resolve_repo(&projects, "Orphan").is_err());
    }
}
