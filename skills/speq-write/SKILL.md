---
name: speq-write
description: Use when drafting, updating, or reviewing Speq product spec documents in a specs repository cloned with `speq clone` (often `./specs`, with `llms.txt` and `epics/`) - product briefs, PRDs, user stories, acceptance criteria, engineering tasks, epic architecture or tech discovery, and decision records. Also use when a new brief needs a PRD, a PRD needs stories, AC, and tasks, someone asks whether a story is ready for engineering, a new product needs its `<product>-specs` repository set up, or existing docs in an implementation repository should move into Speq.
compatibility: Requires the speq CLI (signed in with `speq login`) and Git. Documents live in a repository cloned with `speq clone`.
metadata:
  version: "__SPEQ_VERSION__"
---

# Writing Speq spec documents

A Speq document turns the documents upstream of it into the next one in the team flow: brief → PRD → stories → acceptance criteria → tasks. Every rule, number, name, and date in it comes from a source. Anything the sources do not settle becomes an open question. A document is final once its owner pushes it; there is no separate draft or approval state.

## 1. Locate the repository and the user's permissions

1. The specs are a Git repository cloned with `speq clone`. Its root holds `llms.txt` and `epics/`, and `git config --get speq.address` prints the Speq project there. From an implementation repository it is usually `./specs` (another folder if the user set `--specs-dir` or `SPEQ_SPECS_DIR`). A plain clone of the `<product>-specs` repository from GitHub has the same layout in its root and no `speq.address`: write there the same way, and let the user commit and push with `git`. Every path below is relative to that root. For a new product without a specs repository yet, start with `references/new-product.md`.
2. Run `speq pull` there, so you work on the latest `main`. If it stops with `CONFLICT`, stop and ask the user to resolve it first.
3. Read `references/conventions.md` (in this skill's directory). It defines the structure, file names, IDs, frontmatter, permissions, and how to treat sources outside the repository. `speq push` refuses any path that does not follow it.
4. Read the user's roles and write permissions as described in `conventions.md` §Membaca izin yang berlaku. Use them in step 2.

## 2. Pick the task

| Request | Role that writes it | Read first | Write | Reference |
| --- | --- | --- | --- | --- |
| Tidy raw notes into a brief | `po` | the notes, existing `briefs/` | `epics/<epic>/briefs/br-NNN-<judul>.md` | `references/brief.md` |
| Brief → PRD | `analyst` | every brief of the epic, tech discovery in `architecture.md`, current `prd.md` | `epics/<epic>/prd.md` | `references/prd.md` |
| PRD → stories | `analyst` | `prd.md` §5–7, briefs | `stories/st-NNN-<nama>/README.md`, one per story in PRD §5 | `references/story.md` |
| Story → AC | `analyst`, `qa` | story README, PRD §6–7 | `stories/st-NNN-<nama>/acceptance-criteria.md` | `references/acceptance-criteria.md` |
| Story → tasks | `engineer` | the files `speq context <epic>/<story>` prints, the implementation code | `stories/st-NNN-<nama>/tasks/<jenis>-NNN-<judul>.md` | `references/task.md` |
| Tech discovery / architecture | `engineer` | brief and PRD questions, `prd.md`, the implementation code | `epics/<epic>/architecture.md` or `architecture.md` | `references/architecture.md` |
| Record a decision | `analyst`, `qa`, `ui_ux`, `engineer` | the related question and documents | `epics/<epic>/decisions/dr-NNN-<judul>.md` | `references/decision.md` |
| Review a document, or check handoff readiness | any role | the documents in question | a report; edits only on request | `references/review.md` |
| Set up a new product | platform admin | the product description from the user | `README.md`, `llms.txt`, `architecture.md`, `design/DESIGN.md`, first brief | `references/new-product.md` |
| Move existing docs into Speq | any role (`project_admin` for all paths) | `docs/`, ADRs, and READMEs in the implementation repository, plus its code | the documents the inventory maps to | `references/import.md` |

Before writing a file, check its path against the user's permissions from step 1:

- **Allowed:** write it.
- **Not allowed:** do not write it. Tell the user which role writes it. Write it only if the user asks for a draft for that role; then leave it uncommitted and say who must push it.

When the user asks for the whole chain, write it in order: PRD, then stories, then AC, then tasks. Stop after the PRD and again after the stories, and show the scope (story list, blocking questions) before continuing. Later documents inherit every scope mistake.

## 3. Write

1. **Cite sources.** Each behavior, business rule, quality requirement, and goal names its source, for example `*(brief: Problem 2, DoD 3)*`. In the PRD, mark Analyst inferences with *(Analyst)*.
2. **Turn gaps into questions.** A missing number, rule, name, owner, or date becomes an open question in the checklist format of `conventions.md` §Pertanyaan terbuka. Write `menunggu: <ringkasan>` where the answer is needed. Never fill a gap with a plausible guess.
3. **Use only four IDs:** `st-NNN`, `AC-NN`, `BR-NN`, and the task ID (`be-001`). Do not create FR, NFR, or numbered OQ IDs. A new number is the highest existing number + 1. Never renumber or reuse a number.
4. **Keep the chain traceable:** every behavior has a source, every AC names what it tests, every task names its AC, and every AC is covered by a task.
5. **Frontmatter only on briefs and PRDs**, with only `version` (and `briefs` on a PRD). Follow the step-by-step rules and examples in `conventions.md` §Frontmatter. Every other document has no frontmatter.
6. **Sources outside the repository** follow the table in `conventions.md` §Sumber di luar repository. Ask the user when you cannot tell whether data defines behavior or is content another team maintains.
7. **Language.** Write in Bahasa Indonesia and keep the team's English terms (AC, endpoint, story, redirect). Write plain, specific sentences: who does what, with which value.
8. **Update `llms.txt`:** one line, with a link and a short summary, for each new document.

## 4. Check before handing back

Run the checklist in `references/review.md` §Daftar periksa per dokumen on every file you wrote, and fix what it finds.

## 5. Hand back

1. Run `speq status`. Your files should show as new or modified.
2. Do not commit or push unless the user asks. A push makes the documents final for everyone, so the user decides when.
3. When the user asks to publish, follow the `speq-cli` skill: `speq add <files>`, `speq commit -m "docs: <ringkasan>"`, `speq pull`, and `speq push`. It covers rejected pushes and conflicts. A `403` means the user's role may not write one of the paths, and nothing was written.
4. Reply in Bahasa Indonesia with:
   - the files written,
   - the new IDs (stories, AC, BR, tasks),
   - the open questions added, blocking ones first,
   - the assumptions the user must confirm,
   - the files another role must write or push.

## Common mistakes

| Mistake | Correct form |
| --- | --- |
| `FR-01`, `NFR-02`, or `OQ-03` in a document | Behaviors as a numbered list in the story, quality requirements as a list in PRD §7, open questions as a checklist |
| `id`, `type`, `owner`, `status`, or other frontmatter keys | Only `version` (brief, PRD) and `briefs` (PRD) |
| `briefs: [mcat-br-001]` in a PRD | `briefs: br-001@3`, built as in `conventions.md` §Frontmatter |
| Editing a brief or PRD without raising `version` | Old value + 1 on every content change |
| A plausible number fills a gap ("maksimal 3 kali", "5 detik") | An open question, plus `menunggu: <ringkasan>` where it is needed |
| AC says "berjalan dengan baik" or "sesuai desain" | Given/When/Then with an observable result, plus failure cases |
| One task covers backend and frontend | Split by layer: one task is one PR, about two days of work |
| A task introduces a business rule | A question to the `analyst` in story section 6 |
| All tasks in one `tasks.md` | One file per task in `tasks/`, for example `tasks/be-001-session-api.md`. `speq push` refuses `tasks.md` |
| A link to a sheet that defines behavior | The data copied as a table in the story, with its origin and date |
| Folder `stories/Login Page/` or `st-1-login` | `stories/st-001-login-page/`: lowercase, three-digit number |
