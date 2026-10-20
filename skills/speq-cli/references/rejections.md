# Pushes the API rejects

Git prints `! [remote rejected] main -> main (<reason>)`, and the `speq: ...` lines above it give the details. One problem rejects the whole push, and nothing is written to the remote. Fix the cause in the unpushed commits, then push again.

| Reason | Cause | Fix |
| --- | --- | --- |
| `your role may not change every document in this push` | A path is outside the user's role; a `speq: not allowed: <action> <path>` line names it | Tell the user which path and which role writes it. If the user agrees to drop that change: `speq restore --source origin/main <path>`, then `speq commit --amend` (last commit) or a new commit. Otherwise someone with that role pushes it, or a platform admin grants access |
| `commits must be authored as <email>` | A commit carries another email | Check `speq config user.email`. If it is not the user's Speq email, ask the user for it and run `speq config user.email <email>`. Then `speq commit --amend --no-edit --reset-author` for the last commit, or `speq rebase origin/main --exec "git commit --amend --no-edit --reset-author"` for every unpushed commit |
| `too many commits; ... speq push --squash` | More than 20 commits are waiting | With the user's consent, `speq push --squash` (folds them into one commit; refuses while changes are staged or when the branch lacks the latest `origin/main`). Or push in steps: `speq push origin <commit>:main`, oldest first |
| `history on the Speq remote cannot be rewritten` | A force push | `speq pull`, then `speq push` without `--force` |
| `the Speq remote only has main` | A push to another branch, or a tag | `speq push` from a branch created from `origin/main`, or `speq push origin <branch>:main` |
| `... is not a Speq document path` | A commit touches a non-document file (`.github/`, `tasks.md`, `notes.md`, ...) | `speq restore --source origin/main <path>` (or `speq rm --cached <path>` for a new file), then amend. Document paths are in the `speq-write` conventions |
| `... must be a regular file` | A document is a symlink or executable | Replace it with a plain file (`chmod -x <path>`), then amend |
| `not logged in to Speq; run speq login` | The token expired or the user logged out | Ask the user to run `speq login`, then push again |

Amending only works on commits that are not pushed yet; that is always the case here, because a rejected push writes nothing.

## When `origin` no longer points at the project

Every Git command through speq then ends with a warning and the exact commands that restore it, for example:

```text
speq: origin no longer points to this Speq project, so fetch, pull, and push may fail. Restore it with:
  speq remote set-url --push origin speq::https://speq-api.example.com/v1/projects/<id>
```

Run them as printed. Pointing `origin` elsewhere never gets around the push rules.
