---
name: speq-review
description: Use when reviewing Speq spec documents (brief, PRD, story, acceptance criteria, task, architecture, decision record) before handoff, before `speq push`, or after drafting them - and whenever a document reads as AI-generated, generic, padded, vague, or not professional enough for the team.
---

# Reviewing Speq spec documents

A good spec document lets the next role act without asking anything: the Analyst writes stories from the PRD, QA tests from the AC, and the Engineer builds from the tasks. It also reads like it was written by someone on the team. A review has two passes, **substance** and **writing**, and produces a findings report. Edit files only when the user asks.

Folder structure, IDs, frontmatter, and statuses are defined in `../speq-write/references/conventions.md` (relative to this skill's directory). Read it first.

## Pass 1: substance

**Every document**
- Frontmatter has all keys for its type, a valid `status`, and a current `updated`. A brief and a PRD have an integer `version`. The path and file name follow the conventions.
- Relative links point to files that exist.
- Every rule, number, name, and date has a source (a brief section, DoD, FR/BR, or OQ). Anything unsourced and undecided is a finding.
- No leftovers: `<...>`, `TBD`, `TODO`, `[judul task]`, or HTML template comments.

**By type**

| Document | Check |
| --- | --- |
| Brief | Problem has evidence. Definition of done is observable. Open questions name who is asked. |
| PRD | `briefs` lists every brief of the epic as `br-NNN@<version>` (`briefs: br-001@3, br-002@1`). Any other format (`[mcat-br-001]`) is unreadable for the Web app: `major`. A brief whose current `version` is higher than the recorded one, a brief not listed, or a listed brief that no longer exists means the PRD is stale: `major`, naming the brief. Problems are numbered. Goals point to problems. Metrics are measurable or an OQ. The story status table exists. Blocking OQs are listed. No FR is defined in the PRD. Out of scope is stated. |
| Story | User story has role, capability, and benefit. Each FR is one testable behavior with a source. FR numbers are unique in the epic. Out of scope is stated. `needs_design` matches section 7. |
| Acceptance criteria | Given/When/Then. Each *Then* is observable. Each FR/NFR of the story has an AC. Failure and edge cases exist. No implementation detail. |
| Task | Names its AC. One layer. Fits in one PR. "Selesai bila" is checkable. No new business rule. `depends_on` and `waiting_on` point to real IDs. |
| Architecture | Tech discovery answers cite evidence (file, endpoint, document). Unverified claims are marked. Decisions link to a DR. |
| Decision Record | Context, options, decision, and impact are present. `accepted` only with an owner and `decided_at`. |

**Across documents** (when reviewing a story, an epic, or before handoff)
- An FR without an AC, an AC without a task, or a task that cites a missing AC.
- A story at `ready-for-engineering` while it still has a blocking OQ.
- The same term used with different meanings, or one concept with several names.

## Pass 2: writing

The document must read as a professional team document: specific, plain, and short. Flag these patterns:

| Pattern | Example | Fix |
| --- | --- | --- |
| Generic opener or closer | "Dalam era digital yang terus berkembang, ...", "Kesimpulannya, ..." | Start with the fact. Delete the closer. |
| Self-reference | "Dokumen ini akan menjelaskan ..." | Delete it. The heading already says this. |
| Inflated or empty words | "memastikan pengalaman yang mulus", "solusi komprehensif", "robust", "seamless", "secara efektif dan efisien", "mengoptimalkan" with no measure | State what changes, for whom, and by how much. |
| Unmeasurable requirement | "cepat", "mudah digunakan", "user-friendly", "aman", "responsif" | Use a number and a condition, or `menunggu OQ-NN`. |
| Hedging in requirements | "sebaiknya", "dapat dipertimbangkan", "mungkin perlu" | Use "harus", or turn it into an OQ. |
| Rule of three | "cepat, aman, dan andal" | Keep only the items you can verify. |
| Padding | "Seperti yang telah disebutkan sebelumnya", a summary that repeats the previous paragraph | Delete it. |
| Wordy constructions | "Hal ini memungkinkan pengguna untuk ...", "Dengan demikian", "dilakukan oleh sistem" | Use subject + verb: "Pengguna dapat ...", "Sistem menyimpan ...". |
| Over-formatting | Bold on many phrases, emoji, a heading for one line, nested bullets for prose, a one-row table | Bold only IDs and key labels. Write prose as sentences. |
| Em-dash as connector | "Sistem menyimpan data — lalu mengirim email — ke peserta." | Use a comma or a period. The "st-001 — Judul" heading format is team convention and stays. |
| Speculation outside scope | "Ke depannya dapat dikembangkan dengan AI ..." | Delete it, or move it to Out of scope. |
| Inconsistent terms | "peserta", "user", and "kandidat" for the same role | Use one term per concept, matching the glossary. |
| Mixed language | English sentences inside an Indonesian document | Use Indonesian. Keep English only for team terms (FR, AC, endpoint). |

## Report format

Write the report in Bahasa Indonesia, using this structure:

```markdown
## Review: <path>

**Verdict:** Siap | Perlu revisi kecil | Perlu revisi besar. <Satu kalimat alasan.>

### Substansi
- **[blocker]** L<n>: <masalah>. Usulan: <perbaikan konkret>.
- **[major]** L<n>: ...

### Gaya tulisan
- **[minor]** L<n>: "<kutipan persis>" → "<kalimat usulan>".

### Pertanyaan untuk pemilik dokumen
- <Hal yang hanya bisa diputuskan pemilik dokumen.>
```

- **Severity:**
  - `blocker`: the document cannot be handed off. Examples: an untestable AC, an invented rule, a missing AC for an FR, an ignored blocking OQ.
  - `major`: the next role would have to ask.
  - `minor`: wording or formatting.
- Order findings by severity. Quote the exact text and give its line number.
- Group repeats into one finding, for example: `"memastikan" muncul 7× (L4, L9, L15, ...)`.
- Include findings only. No praise. List at most 20; when there are more, report the count for each pattern.
- **Applying fixes (only when the user asks):**
  - Fix writing and mechanical issues directly.
  - Never decide substance. A missing rule, number, or owner becomes a question for the owner, not a value you choose.
  - After editing, update `updated` in the frontmatter, and increment `version` if the document has one.
