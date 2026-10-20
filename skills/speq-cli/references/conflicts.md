# Resolving conflicts

`speq pull` rebases the unpushed commits onto `main` (`pull.rebase=true`), puts uncommitted changes aside and restores them afterwards (`rebase.autoStash=true`), and writes conflict markers with the base version (`merge.conflictStyle=diff3`).

## Push rejected because the remote moved

```text
 ! [rejected]        main -> main (fetch first)
```

or `(non-fast-forward)`. Nothing was written. Run `speq pull`. If it ends without `CONFLICT`, the two sides changed different files or lines: run `speq push` again (after the user's go-ahead if they have not already asked for the push).

## Pull stops in the middle of a rebase

```text
CONFLICT (content): Merge conflict in epics/epic-payment/prd.md
error: could not apply 1a2b3c4... docs: add the refund rule
```

The file holds:

```text
<<<<<<< HEAD
the remote version (already on main)
||||||| parent of 1a2b3c4 (docs: add the refund rule)
the version from before either side changed it
=======
the user's version
>>>>>>> 1a2b3c4 (docs: add the refund rule)
```

During a rebase `HEAD` is the **remote** version and the last block is **the user's commit**.

### The user decides every block

Which version a document keeps is the user's decision, even when the merge looks obvious: the other side may be a
decision the user has not seen yet. Never edit a conflicted file, run `speq add`, or run `speq rebase --continue`
before the user has chosen for every block of the current commit.

1. Gather the conflicts: `speq status` (files marked *both modified*), and in each file every block from `<<<<<<<` to
   `>>>>>>>`. Read the whole section around each block, not only the marked lines.
2. Find who changed the remote side and why: `speq log --oneline -5 HEAD -- <file>` (during a rebase `HEAD` holds the
   remote commits). The commit being replayed is named in the `error: could not apply <sha>... <subject>` line.
3. Present one block at a time, in the user's language, and wait for the answer:

   ```text
   Conflict 1 of 3 — epics/epic-payment/prd.md, §6 Business rules (lines 42–50)
   Your commit: 1a2b3c4 docs: add the refund rule
   Remote change: 9f8e7d6 docs: cap refunds at 30 days (Ana)

   Before either change:
     <base block>
   Remote (already on main):
     <HEAD block>
   Yours:
     <your block>

   What differs: <one sentence, for example "remote limits refunds to 30 days, yours to 14 days">
   Suggested: <option and why>

   Which one should the document keep?
     1. Remote version
     2. Your version
     3. Both, combined as: <the exact combined text>
     4. Something else (write it, or tell me what to change)
   ```

   - Quote the blocks exactly; shorten only unchanged lines around them (`…`).
   - Offer option 3 only when both changes can stand together, and show the exact text it would write.
   - When the sides disagree on meaning (a number, rule, owner, date, or decision), say so in *What differs* and do not
     suggest one: it is a product decision, and the other author may need to be asked.
   - If the agent has a tool for asking the user a question with options, use it; otherwise ask in plain text.
4. With many blocks in one file, the user may answer once for the whole file ("remote for all", "mine for all"). Use
   `speq checkout --ours <file>` for the remote version or `speq checkout --theirs <file>` for the user's version (the
   names are reversed during a rebase), and only after that answer.
5. Write each chosen version exactly, remove every marker line (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`), and check
   none is left: `speq diff --check`. If a brief or PRD changed on both sides, set its frontmatter `version` to the
   remote value + 1 (the `speq-write` conventions).
6. Show the result before going on: `speq diff --staged` after `speq add <file>`, or the resolved section. Ask
   "Continue the rebase with this?" and wait.
7. On yes: `speq rebase --continue`. If the next commit conflicts too, start again from step 1 for that commit.
8. When the rebase finishes, push only if the user already asked for the push; otherwise say the branch is ready.

If the user wants to stop, wants to ask the other author first, or does not answer, leave the rebase as it is and tell
them where things stand: `speq status` shows the conflicted files, `speq rebase --continue` resumes after resolving,
and `speq rebase --abort` returns to the state before the pull (nothing is lost; the commits are still on the branch).
Never abort on your own.

## Conflict while restoring uncommitted changes

```text
Applying autostash resulted in conflicts.
Your changes are safe in the stash.
```

The commits are done; only the uncommitted changes collided, and they are still in the stash.

1. Let the user decide every block, as in [The user decides every block](#the-user-decides-every-block). Here
   `Updated upstream` is the committed version and `Stashed changes` is the user's uncommitted work.
2. `speq restore --staged <file>` (or `speq add <file>` to stage them).
3. Only when the user confirms the result is right: `speq stash drop`. To start over instead, `speq checkout HEAD -- <file>` and `speq stash pop`.

## Pull merges instead of rebasing

When `pull.rebase` was set to `false`, a conflict shows `Automatic merge failed; fix conflicts and then commit the result.` Let the user decide every block the same way (`HEAD` is then the user's side, and the commit is `speq commit` after their go-ahead), then `speq add <file>`, `speq commit`, and `speq push`. Cancel with `speq merge --abort`. Merge commits are pushed like any other; the API judges a merge only by what it adds on top of the remote.

## Diverged

`speq status` saying *have diverged* after `speq fetch` means both sides have commits the other lacks: `speq pull`, resolve as above, then `speq push`.
