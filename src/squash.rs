//! `speq push --squash`: turn the commits not yet on the remote into one, then push. Opt-in only (the push error
//! for too many commits suggests it); it rewrites local history the way GitHub's "Squash and merge" does.
use crate::error::{Error, Result};
use crate::git::Git;
use std::path::Path;

pub const UPSTREAM: &str = "refs/remotes/origin/main";

/// The squashed commit's message: a summary line, then every original message's subject in order.
pub fn message(subjects: &[String]) -> String {
    let mut text = format!("Squash {} commits\n\n", subjects.len());
    for subject in subjects {
        text.push_str(&format!("* {subject}\n"));
    }
    text
}

/// How many commits were folded into one; 0 or 1 means nothing had to change.
pub fn squash(git: &Git, dir: Option<&Path>) -> Result<usize> {
    if !git.succeeds(dir, ["rev-parse", "--verify", "-q", UPSTREAM])? {
        return Err(Error::invalid(
            "origin/main is unknown here; run `speq fetch` first",
        ));
    }
    if !git.succeeds(dir, ["merge-base", "--is-ancestor", UPSTREAM, "HEAD"])? {
        return Err(Error::conflict(
            "your branch does not contain the latest origin/main; run `speq pull` first so the squash cannot undo anyone's work",
        ));
    }
    if !git.succeeds(dir, ["diff", "--cached", "--quiet"])? {
        return Err(Error::conflict(
            "there are staged changes; commit or unstage them first so they are not folded in unnoticed",
        ));
    }
    let range = format!("{UPSTREAM}..HEAD");
    let subjects: Vec<String> = git
        .text(dir, ["log", "--reverse", "--format=%s", &range])?
        .lines()
        .map(str::to_string)
        .collect();
    if subjects.len() < 2 {
        return Ok(subjects.len());
    }
    let before = git.text(dir, ["rev-parse", "HEAD"])?.trim().to_string();
    git.text(dir, ["reset", "-q", "--soft", UPSTREAM])?;
    let committed = git.output(dir, ["commit", "-q", "-m", &message(&subjects)])?;
    if !committed.status.success() {
        // Put the branch back exactly where it was: nothing is lost.
        git.text(dir, ["reset", "-q", "--soft", &before])?;
        return Err(Error::Other(format!(
            "the squash commit could not be made, the branch is unchanged: {}",
            crate::git::git_failure(&committed)
        )));
    }
    Ok(subjects.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_lists_every_subject_in_order() {
        assert_eq!(
            message(&["docs: one".into(), "docs: two".into()]),
            "Squash 2 commits\n\n* docs: one\n* docs: two\n"
        );
    }
}
