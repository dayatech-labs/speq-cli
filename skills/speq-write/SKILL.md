---
name: speq-write
description: Use when drafting or updating Speq product spec documents in a specs repository cloned with `speq clone` (often `./specs`, with `llms.txt` and `epics/`) - a product brief, PRD, user stories, acceptance criteria, engineering tasks, epic architecture or tech discovery, or a decision record. Also use when a new brief arrives and needs a PRD, a PRD needs to be broken into stories, AC, and tasks, a new product needs its `<product>-specs` repository set up, or existing docs (`docs/`, ADRs, READMEs) in an implementation repository should move into Speq.
---

# Drafting Speq spec documents

A draft turns the documents upstream of it into the next document in the team flow: brief → PRD → stories → acceptance criteria → tasks. The owning role reviews and approves it. Every rule, number, name, and deadline in a draft comes from a source document. Anything the sources do not settle becomes an open question.

## 1. Locate the specs repository

1. The specs are a Git repository cloned with `speq clone`. Its root holds `llms.txt` and `epics/`, and `git config --get speq.address` prints the Speq project there. From an implementation repository it is usually `./specs`. Every path below is relative to that root. For a new product without a specs repository yet, start with `references/new-product.md`.
2. Run `speq pull` there, so you draft on the latest `main`. If it stops with `CONFLICT`, stop and ask the user to resolve it first.
3. Read `references/conventions.md` (in this skill's directory). It defines the folder structure, file names, IDs, frontmatter, and statuses. `speq push` refuses any path that does not follow it.

## 2. Pick the document

| Request | Owner | Read first | Write | Reference |
| --- | --- | --- | --- | --- |
| Tidy raw notes into a brief | PO | the notes, existing `briefs/` | `epics/<epic>/briefs/br-NNN-<judul>.md` | `references/brief.md` |
| Brief → PRD | Analyst | every brief of the epic, `architecture.md` tech discovery, current `prd.md` | `epics/<epic>/prd.md` | `references/prd.md` |
| PRD → stories | Analyst | `prd.md` §5, briefs | `stories/st-NNN-<nama>/README.md`, one per story in PRD §5 | `references/story.md` |
| Story → AC | Analyst + QA | story README, PRD §6–7 | `stories/st-NNN-<nama>/acceptance-criteria.md` | `references/acceptance-criteria.md` |
| Story → tasks | Engineer (Analyst may draft) | the files `speq context <epic>/<story>` prints, the implementation code if present | `stories/st-NNN-<nama>/tasks/<jenis>-NNN-<judul>.md` | `references/task.md` |
| Technical draft / tech discovery | Engineer | brief OQs, `prd.md`, the implementation code | `epics/<epic>/architecture.md` | `references/architecture.md` |
| Significant decision | decision owner + Analyst | the related OQ and documents | `epics/<epic>/decisions/dr-NNN-<judul>.md` | `references/decision.md` |
| Set up a new product | platform admin | the product description from the user | `README.md`, `llms.txt`, `architecture.md`, `design/DESIGN.md`, first brief | `references/new-product.md` |
| Move existing docs into Speq | any role (`project_admin` for all paths) | `docs/`, ADRs, and READMEs in the implementation repository, plus its code | the documents the inventory maps to | `references/import.md` |

When the user asks for the whole chain, write it in order: PRD, then stories, then AC, then tasks. Stop after the PRD and again after the stories, and show the user the scope (story list, FR ranges, blocking OQs) before continuing. Later documents inherit every scope mistake.

## 3. Write the draft

Every draft follows these rules:

1. **Cite sources.** Each FR, BR, NFR, and goal names its source, for example `*(Must · brief: Problem 2, DoD 3)*`. In the PRD, mark Analyst inferences with *(Analyst)*.
2. **Turn gaps into open questions.** If a number, rule, name, owner, or date is missing, add an `OQ-NN` to `prd.md` §10, or to story section 6 when the PRD is not part of this task. Write `menunggu OQ-NN` at the place the answer is needed.
3. **Keep IDs stable.** New IDs continue from the highest existing number. Never renumber or reuse an ID.
4. **Keep traceability complete.** A story lists its FR/NFR. Each AC names the FR it tests. Each task names its AC. Every AC is covered by at least one task.
5. **Set frontmatter.**
   - Use `status: draft`, `drafted_by: AI`, and `updated:` set to today's date.
   - When you change the content of a document that has `version`, increment it by 1. A new brief or PRD starts at `version: 1`.
   - A PRD records the brief versions it is based on, for example `briefs: br-001@3, br-002@1`.
   - Leave `approved_by`, `approved_at`, and other roles' statuses unchanged.
6. **Stay in your row.** Edit only the files in the row you picked, plus `llms.txt`. If a change belongs in another role's document, list it in your reply.
7. **Language.** Write in Bahasa Indonesia. Keep the team's English terms (FR, AC, endpoint, redirect, story). Write plain, specific sentences: who does what, with which value.
8. **Update `llms.txt`.** Add one line (link plus a short summary) for each new document under the owning role's section.

## 4. Check and hand back

1. Apply the **speq-review** skill checklist to every file you wrote and fix what it finds. If that skill is not installed, check rules 1–4 above yourself.
2. Run `speq status`. Your files should show as new or modified. Do not commit or push unless the user asks, because the owning role reads the draft first.
3. When the user asks to publish, run `speq add <files>`, `speq commit -m "docs: <ringkasan>"`, and `speq push`. Pushes go to `main` only. A `403` means the user's role may not write one of the paths, and nothing was written. A `fetch first` rejection means someone pushed first: run `speq pull`, then `speq push` again.
4. Reply in Bahasa Indonesia with:
   - the files written,
   - the new IDs (FR, AC, tasks),
   - the OQs added, with blocking ones first,
   - the assumptions the owner must confirm.

## Common mistakes

| Mistake | Correct form |
| --- | --- |
| FR defined in the PRD | FR lives in the story README. The PRD shows FR ranges per story. |
| A plausible number fills a gap ("maksimal 3 kali", "5 detik") | Write `menunggu OQ-NN` and add the question. |
| AC says "berjalan dengan baik" or "sesuai desain" | Given/When/Then with an observable result. Include failure cases. |
| One task covers backend and frontend | Split by layer. One task is one PR, about two days of work. |
| A task introduces a business rule | Raise it as a question to the Analyst in story section 6. |
| All tasks in one `tasks.md` | One file per task in `tasks/`, for example `tasks/be-001-session-api.md`. `speq push` refuses `tasks.md`. |
| `briefs: [mcat-br-001]` in a PRD | `briefs: br-001@3`: the Web app reads only `id@version` pairs. |
| Folder `stories/Login Page/` or `st-1-login` | `stories/st-001-login-page/`: lowercase, three-digit number. |
| The whole brief copied into the PRD | Summarize it and link the brief. |
