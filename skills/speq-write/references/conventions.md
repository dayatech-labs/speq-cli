# Konvensi dokumen Speq

Berlaku untuk setiap dokumen di repository specs hasil `speq clone` (dari repository implementasi biasanya `./specs`). Semua path di bawah relatif terhadap root repository itu.

## Struktur

```text
README.md                          # Analyst: gambaran produk dan indeks epic
llms.txt                           # Semua role: indeks dokumen per section
architecture.md                    # Engineer/Architect: arsitektur lintas epic
design/DESIGN.md                   # UIUX: aturan desain produk
design/components/<nama>.html      # UIUX: prototipe komponen bersama
epics/<epic>/
├── briefs/br-NNN-<judul>.md       # PO: brief awal dan brief tambahan
├── decisions/dr-NNN-<judul>.md    # Pemilik keputusan + Analyst
├── prd.md                         # Analyst: why dan what tingkat epic, BR, NFR, OQ
├── architecture.md                # Engineer: tech discovery dan arsitektur epic
└── stories/st-NNN-<nama>/
    ├── README.md                  # Analyst: user story, FR, aturan story
    ├── acceptance-criteria.md     # Analyst + QA: hasil yang dapat diuji
    ├── DESIGN.md                  # UIUX, opsional
    ├── layout.html                # UIUX, opsional: prototipe halaman
    └── tasks/<jenis>-NNN-<judul>.md   # Engineer: satu file per task (be, fe, qa, ...)
```

- Nama folder dan file memakai huruf kecil, angka, dan `-` saja. Pengecualian: `README.md` dan `DESIGN.md` ditulis kapital.
- Nomor memakai tiga digit (`001`, `002`, ..., `010`). Nomor tidak pernah dipakai ulang, juga setelah dokumennya dihapus.
- Nama setelah nomor paling panjang 50 karakter untuk folder story, dan 80 karakter untuk file brief, decision, dan task.
- Jenis task ditulis 2–8 huruf kecil: `be`, `fe`, `qa`, atau jenis lain yang memang dipakai tim. Nomor task dihitung per jenis di dalam satu story.
- Dokumen harus berupa teks UTF-8. `speq push` menolak seluruh push sebelum mengirim apa pun bila ada satu path di luar pola ini, misalnya `tasks.md`, `notes.md`, atau `templates/`.

**Epic** adalah area produk jangka panjang (`mcat`, `landing-page`), dan tidak dibuat ulang untuk setiap permintaan. **Brief** adalah satu permintaan bisnis yang masuk ke epic. Permintaan berikutnya pada area yang sama menjadi `br-002`, `br-003`, dan seterusnya.

## ID dan traceability

| ID | Letak | Rujukan global |
| --- | --- | --- |
| `BR-NN` | Business rule lintas story, di `prd.md` | `mcat-BR-02` |
| `NFR-NN` | Non-functional requirement, di `prd.md` | `mcat-NFR-01` |
| `OQ-NN` | Open question, di `prd.md` | `mcat-OQ-05` |
| `FR-NN` | Functional requirement, di `README.md` story; nomor unik di seluruh epic | `mcat-FR-04` |
| `st-NNN` | Story | `mcat-st-003` |
| `AC-NN` | Acceptance criteria, di `acceptance-criteria.md`; nomor per story | `mcat-st-003-AC-02` |
| `be-NNN`, `fe-NNN`, `qa-NNN` | Task, satu file per task | `mcat-st-003-be-001` |

Rantai yang harus bisa ditelusuri: brief → FR → story → AC → task → PR/hasil uji.

- Setiap story mencantumkan FR/NFR yang dipenuhinya (`requirements` di frontmatter).
- Setiap AC menyebut FR/NFR yang diujinya.
- Setiap task menyebut AC yang diselesaikannya, dan setiap AC ditutup minimal satu task.
- Nomor baru melanjutkan nomor tertinggi yang sudah ada di epic (FR, BR, NFR, OQ) atau di story (AC, task). Cek dulu dokumen yang ada sebelum memberi nomor.

## Frontmatter

Setiap dokumen diawali frontmatter YAML.

| Dokumen | Key |
| --- | --- |
| Brief | `version`, `id: <epic>-br-NNN`, `type: brief`, `epic`, `owner: PO`, `status`, `requester`, `urgency`, `needed_by`, `date`, `updated` |
| PRD | `version`, `briefs: br-001@3, br-002@1`, `id: <epic>-prd`, `type: prd`, `epic`, `owner: Analyst`, `status`, `approved_by`, `approved_at`, `updated` |
| Story | `id: <epic>-st-NNN`, `type: story`, `epic`, `owner: Analyst`, `status`, `requirements: [FR-NN, NFR-NN]`, `needs_design`, `updated` |
| Acceptance criteria | `id: <epic>-st-NNN-ac`, `type: acceptance-criteria`, `epic`, `story: <epic>-st-NNN`, `owner: Analyst, QA`, `status`, `updated` |
| Task | `id: <epic>-st-NNN-<jenis>-NNN`, `type: task`, `epic`, `story`, `owner: Engineer`, `drafted_by`, `status`, `ac: [AC-NN]`, `depends_on`, `waiting_on`, `updated` |
| Architecture | `id: <epic>-architecture`, `type: architecture`, `epic`, `owner: Engineer`, `status`, `updated` |
| Decision Record | `id: <epic>-dr-NNN`, `type: decision`, `epic`, `owner: <role pemilik keputusan>`, `status`, `decided_at`, `updated` |

`updated` diisi tanggal hari ini (`YYYY-MM-DD`) setiap kali dokumen diubah. `approved_by` dan `approved_at` hanya diisi PO.

### Versi brief dan PRD

- `version` adalah bilangan bulat. Brief dan PRD baru dimulai dari `version: 1`. Web app Speq menaikkannya satu angka setiap kali dokumen disimpan, dan `version` yang tidak ada dihitung 0. Perubahan lewat Git (`speq commit`) tidak dinaikkan otomatis, jadi naikkan `version` sendiri setiap kali isi brief atau PRD berubah.
- `briefs` di PRD mencatat versi brief yang sudah direview untuk PRD itu. Formatnya `br-NNN@<version>`, dipisah koma, misalnya `briefs: br-001@3, br-002@1`. `br-NNN` diambil dari nama file brief. Web app hanya membaca format ini; bentuk lain seperti `[mcat-br-001]` dianggap belum mencatat brief apa pun.
- Isi `briefs` hanya setelah isi PRD benar-benar dicocokkan dengan brief versi tersebut. Memperbaiki salah ketik di PRD tidak mengubah `briefs`.
- Kalau `version` brief lebih besar dari angka di `briefs`, ada brief yang belum tercatat, atau ada brief tercatat yang sudah dihapus, berarti PRD tertinggal. Web app menampilkan peringatan untuk kondisi ini. Bandingkan isi brief terbaru dengan PRD sebelum memperbarui `briefs`.

## Status

| Status | Arti |
| --- | --- |
| `draft` | Sedang ditulis pemilik dokumen |
| `in-review` | Menunggu review role terkait |
| `ready-for-design` | Kebutuhan disetujui PO; UIUX dapat mulai |
| `ready-for-engineering` | Desain yang diperlukan selesai; siap handoff |
| `in-progress` | Sedang diimplementasikan |
| `done` | Lolos verifikasi QA |
| `blocked` | Tertahan open question atau dependensi; sebutkan di `blocked_by` |
| `template` | Kerangka yang belum diisi pemiliknya |

Brief memakai `draft` lalu `submitted`. Task memakai `todo` → `in-progress` → `done`, atau `blocked`; task yang belum divalidasi Engineer berstatus `draft`. Decision Record memakai `proposed`, `accepted`, atau `superseded`.

Story tidak boleh berstatus `ready-for-engineering` selama masih ada OQ **blocking** yang belum terjawab.

## Siapa mengubah apa

Izin tulis bawaan API Speq. Semua role juga boleh menulis `llms.txt`, dan `project_admin` boleh menulis semuanya.

| Role | Dokumen |
| --- | --- |
| PO | `epics/*/briefs/**` |
| Analyst | `README.md`, `epics/*/prd.md`, `epics/*/stories/*/README.md`, `epics/*/stories/*/acceptance-criteria.md`, `epics/*/decisions/**` |
| QA | `epics/*/stories/*/acceptance-criteria.md`, `epics/*/decisions/**` |
| UIUX | `design/**`, `epics/*/stories/*/DESIGN.md`, `epics/*/stories/*/layout.html`, `epics/*/decisions/**` |
| Engineer | `architecture.md`, `epics/*/architecture.md`, `epics/*/stories/*/tasks/**`, `epics/*/decisions/**` |

Persetujuan PRD (`approved_by`, `approved_at`) diisi PO lewat Analyst atau Web app. Draf task dari Analyst hanya bisa di-push bila project memberi izin tambahan; tanpa izin itu, serahkan drafnya ke Engineer.

API Speq memutuskan siapa yang boleh menulis path tertentu. Kalau `speq push` ditolak dengan `403`, ada dokumen yang bukan milik role pengguna, dan tidak ada yang tertulis. Push hanya bisa ke `main`.

## `llms.txt`

`llms.txt` adalah indeks bersama, bukan salinan isi dokumen. Setiap role memelihara section miliknya: satu baris per dokumen, berisi tautan relatif dan ringkasan singkat.

| Section | Pengisi | Isi |
| --- | --- | --- |
| Product & requirements | Analyst | `README.md`, PRD, story, acceptance criteria |
| Business briefs | PO | Brief epic |
| Design | UIUX | `design/DESIGN.md`, komponen, desain story |
| Architecture & implementation | Engineer | Arsitektur produk/epic dan task |
| Quality | QA | Kriteria penerimaan dan dokumen pengujian |
| Decisions | Pemilik keputusan | Decision Record yang disepakati |

Contoh baris: `  - [mcat st-003 — Pengerjaan section sesi 1](epics/mcat/stories/st-003-pengerjaan-section/README.md): FR-08–FR-15.`

Tambahkan hanya dokumen yang benar-benar ada, dan perbarui tautan saat file dipindah atau diganti namanya.
