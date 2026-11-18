//! `speq info`: who is signed in on this device, whether Dayaboard is connected, and the roles held per repository.
//! Read from the stored credential and the capability snapshot; only the account's name and email need the API.
use crate::env::Env;
use crate::error::Result;
use crate::session::Session;
use crate::snapshot::now;
use serde::Serialize;
use time::OffsetDateTime;

/// Standing on the whole platform, apart from the roles held in each repository.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlatformRole {
    User,
    PlatformAdmin,
    Superadmin,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Account {
    pub user_id: String,
    /// Absent when the API could not be asked just now; the rest of the report does not depend on it.
    pub email: Option<String>,
    pub name: Option<String>,
    pub platform_role: Option<PlatformRole>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SessionInfo {
    /// There is no refresh: this is the end of the session, after which `speq login` is needed again.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// The Dayaboard key `speq login` stored on this device. Local state only: Dayaboard is not asked.
#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DayaboardState {
    Connected {
        #[serde(with = "time::serde::rfc3339")]
        expires_at: OffsetDateTime,
    },
    Expired {
        #[serde(with = "time::serde::rfc3339")]
        expires_at: OffsetDateTime,
    },
    NotConnected,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProjectRoles {
    pub project_id: String,
    pub display_name: String,
    pub repository_full_name: Option<String>,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct InfoReport {
    pub api_url: String,
    pub account: Account,
    pub session: SessionInfo,
    pub dayaboard: DayaboardState,
    pub projects: Vec<ProjectRoles>,
    /// The project of the specs repository the command ran in, when it ran in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_project_id: Option<String>,
}

pub fn platform_role(is_platform_admin: bool, is_superadmin: bool) -> PlatformRole {
    if is_superadmin {
        PlatformRole::Superadmin
    } else if is_platform_admin {
        PlatformRole::PlatformAdmin
    } else {
        PlatformRole::User
    }
}

/// Classifies the stored Dayaboard key at `at`.
pub fn dayaboard_state(
    key_expires_at: Option<OffsetDateTime>,
    at: OffsetDateTime,
) -> DayaboardState {
    match key_expires_at {
        Some(expires_at) if expires_at > at => DayaboardState::Connected { expires_at },
        Some(expires_at) => DayaboardState::Expired { expires_at },
        None => DayaboardState::NotConnected,
    }
}

pub async fn info(env: &Env, current_project_id: Option<String>) -> Result<InfoReport> {
    let session = Session::new(env);
    // Fails with "not logged in" or "session expired" like every other command, and refreshes a stale snapshot.
    let auth = session.authenticate().await?;
    let snapshot = session.snapshot().await?;
    let credential = session.credential()?;
    // A name and email are nice to have: if the API cannot be reached for them, the report is still true.
    let me = env.client.me(&auth.token).await.ok();

    Ok(InfoReport {
        api_url: env.account(),
        account: Account {
            user_id: snapshot.user_id,
            email: me.as_ref().map(|m| m.email.clone()),
            name: me.as_ref().and_then(|m| m.name.clone()),
            platform_role: me
                .as_ref()
                .map(|m| platform_role(m.is_platform_admin, m.is_superadmin)),
        },
        session: SessionInfo {
            expires_at: credential.expires_at,
        },
        dayaboard: dayaboard_state(credential.dayaboard.map(|k| k.expires_at), now()),
        projects: snapshot
            .projects
            .into_iter()
            .map(|p| ProjectRoles {
                project_id: p.project_id,
                display_name: p.display_name,
                repository_full_name: p.repository_full_name,
                roles: p.roles,
            })
            .collect(),
        current_project_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn the_highest_platform_standing_wins() {
        assert_eq!(platform_role(false, false), PlatformRole::User);
        assert_eq!(platform_role(true, false), PlatformRole::PlatformAdmin);
        assert_eq!(platform_role(true, true), PlatformRole::Superadmin);
        // A superadmin is also a platform admin, but the flag alone is enough to say so.
        assert_eq!(platform_role(false, true), PlatformRole::Superadmin);
    }

    #[test]
    fn a_dayaboard_key_is_connected_until_the_moment_it_expires() {
        let at = now();
        let later = at + Duration::days(3);
        assert_eq!(
            dayaboard_state(Some(later), at),
            DayaboardState::Connected { expires_at: later }
        );
        // Expiring exactly now already counts as expired, as `DayaboardKey::is_expired` does.
        assert_eq!(
            dayaboard_state(Some(at), at),
            DayaboardState::Expired { expires_at: at }
        );
        let earlier = at - Duration::seconds(1);
        assert_eq!(
            dayaboard_state(Some(earlier), at),
            DayaboardState::Expired {
                expires_at: earlier
            }
        );
        assert_eq!(dayaboard_state(None, at), DayaboardState::NotConnected);
    }
}
