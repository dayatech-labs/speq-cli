# Konvensi dokumen Speq

Berlaku untuk setiap dokumen di repository specs hasil `speq clone` (dari repository implementasi biasanya `./specs`). Semua path di bawah relatif terhadap root repository itu.

## Struktur

```text
README.md                          # analyst: gambaran produk dan indeks epic
llms.txt                           # semua role: indeks dokumen
architecture.md                    # engineer: arsitektur lintas epic
design/DESIGN.md                   # ui_ux: aturan desain produk
design/components/<nama>.html      # ui_ux: prototipe komponen bersama
epics/<epic>/
├── briefs/br-NNN-<judul>.md       # po: brief awal dan brief tambahan
├── decisions/dr-NNN-<judul>.md    # analyst, qa, ui_ux, engineer
├── prd.md                         # analyst: why dan what tingkat epic
├── architecture.md                # engineer: tech discovery dan arsitektur epic
└── stories/st-NNN-<nama>/
    ├── README.md                  # analyst: user story, perilaku, aturan story
    ├── acceptance-criteria.md     # analyst dan qa: hasil yang dapat diuji
    ├── DESIGN.md                  # ui_ux, opsional
    ├── layout.html                # ui_ux, opsional: prototipe halaman
    └── tasks/<jenis>-NNN-<judul>.md   # engineer: satu file per task
```

Aturan nama:

- Nama folder dan file memakai huruf kecil, angka, dan `-` saja. Pengecualian: `README.md` dan `DESIGN.md` ditulis kapital.
- Nomor memakai tiga digit (`001`, `002`, ..., `010`). Nomor tidak pernah dipakai ulang, juga setelah dokumennya dihapus. Nomor baru = nomor tertinggi yang ada + 1.
- Nama setelah nomor paling panjang 50 karakter untuk folder story dan 80 karakter untuk file brief, decision, dan task.
- Jenis task ditulis 2–8 huruf kecil: `be` (backend), `fe` (frontend), `qa` (data uji dan verifikasi), atau jenis lain yang dipakai tim. Nomor task dihitung per jenis di dalam satu story.
- Dokumen harus berupa teks UTF-8. `speq push` menolak seluruh push sebelum mengirim apa pun bila ada satu path di luar pola ini, misalnya `tasks.md`, `notes.md`, atau `templates/`.

**Epic** adalah area produk jangka panjang (`mcat`, `landing-page`), tidak dibuat ulang untuk setiap permintaan. **Brief** adalah satu permintaan bisnis yang masuk ke epic. Permintaan berikutnya pada area yang sama menjadi `br-002`, `br-003`, dan seterusnya.

## Nama tabel database

Berlaku untuk setiap nama tabel yang ditulis di dokumen spesifikasi (architecture, task, story) dan untuk migrasi dan query yang dirujuk dokumen itu.

- Nama tabel memakai prefix menurut jenis datanya, lalu `snake_case` tunggal.
- `master_` untuk master data: data referensi, konfigurasi, dan entitas yang dikelola pengguna dan berubah jarang. Contoh: `master_role`, `master_organization`, `master_test`. Tabel relasi mengikuti induknya, mis. `master_role_permission`.
- `trans_` untuk transaksi: data yang dihasilkan oleh aktivitas atau peristiwa dan bertambah seiring pemakaian. Contoh: `trans_session`, `trans_response`, `trans_audit_log`.
- Tabel tanpa prefix tidak boleh dibuat.
- Pengelompokan entitas ke prefix ada di `architecture.md` produk (di repository `engaugemetrics-specs`: §5.5). Entitas baru yang belum terdaftar: tentukan prefix dari dua definisi di atas, lalu tambahkan ke daftar itu.

## Istilah dan ID

Hanya empat jenis ID yang dipakai. Jangan membuat ID lain (FR, NFR, OQ, atau sejenisnya).

| ID | Arti | Letak | Contoh rujukan dari dokumen lain |
| --- | --- | --- | --- |
| `st-NNN` | **Story**: satu hasil yang bernilai bagi satu jenis pengguna dan dapat diserahkan sendiri | Nama folder story | `mcat st-003` |
| `AC-NN` | **Acceptance criteria**: satu hasil yang dapat diuji QA. Nomor per story | `acceptance-criteria.md` | `st-003 AC-02` |
| `BR-NN` | **Business rule**: aturan bisnis yang berlaku untuk lebih dari satu story. Nomor per epic | PRD §6 | `BR-02` |
| `<jenis>-NNN` | **Task**: satu pekerjaan engineering yang selesai dalam satu PR | Nama file task | `st-003 be-001` |

Istilah lain, tanpa ID:

- **Perilaku**: apa yang dilakukan sistem untuk pengguna, ditulis sebagai daftar bernomor di README story ("Perilaku 2"). AC menguji perilaku ini.
- **Kebutuhan kualitas**: syarat yang berlaku umum, misalnya bahasa, browser, perangkat, performa, keamanan. Ditulis sebagai daftar di PRD §7.
- **Pertanyaan terbuka**: hal yang belum diputuskan. Ditulis sebagai checklist (lihat di bawah).

Rantai yang harus bisa ditelusuri: **brief → story → AC → task**.

- Setiap poin perilaku menyebut sumbernya di brief, misalnya `*(brief: Problem 2, DoD 3)*`.
- Setiap AC menyebut yang diujinya: `(Perilaku 2)`, `(BR-01)`, atau `(PRD §7)`.
- Setiap task menyebut AC yang diselesaikannya. Setiap AC ditutup minimal oleh satu task.

## Pertanyaan terbuka

Hal yang belum diputuskan sumber (angka, nama, aturan, pemilik, tanggal) tidak boleh diisi tebakan. Tulis sebagai checklist di section `Pertanyaan terbuka` PRD (atau README story bila hanya menyangkut satu story):

```markdown
## Pertanyaan terbuka

- [ ] Apakah URL halaman detail berubah mengikuti nama baru? (ditanyakan ke: po · **blocking** st-001 · dijawab sebelum 1 Nov)
- [ ] Apakah file contoh laporan diganti di rilis ini? (ditanyakan ke: po)
```

- **Blocking** berarti story yang disebut belum dapat dikerjakan engineering sebelum pertanyaannya dijawab.
- Di tempat jawaban dibutuhkan, tulis `menunggu: <ringkasan pertanyaan> (PRD §10)`, misalnya di AC atau task.
- Setelah dijawab: pindahkan jawabannya ke isi dokumen (aturan, perilaku, atau BR), lalu hapus butir pertanyaannya. Riwayatnya tersimpan di Git.

## Frontmatter

Hanya tiga key yang boleh ada. Jangan menambahkan key lain di dokumen mana pun.

| Key | Dokumen | Arti |
| --- | --- | --- |
| `version` | brief, PRD | Versi isi dokumen. Dibaca Web app Speq. |
| `briefs` | PRD | Brief yang menjadi acuan saat PRD terakhir ditinjau. Dibaca Web app Speq. |
| `ticket-id` | PRD, README story, task | Key tiket Dayaboard dokumen itu, misalnya `DEV-42`. |

Dokumen lain (acceptance criteria, architecture, decision, DESIGN, README produk) tidak memakai frontmatter sama sekali. README story dan task hanya memakai frontmatter bila sudah punya tiket.

### `ticket-id` di PRD, story, dan task

`ticket-id` menandai bahwa dokumen sudah punya tiket di Dayaboard: PRD → tiket Epic, README story → tiket Story, task → tiket Task.

1. Ditulis oleh skill `speq-dayaboard` setelah tiketnya dibuat, atau oleh user. Jangan mengisinya dengan tebakan atau menyalin dari dokumen lain.
2. Isinya hanya key tiket (`DEV-42`), tanpa URL.
3. Menambah atau mengubah `ticket-id` bukan perubahan isi: `version` PRD **tidak** dinaikkan untuk itu.
4. Pertahankan `ticket-id` saat mengubah dokumen. Hapus hanya bila user memintanya.

```markdown
---
ticket-id: DEV-42
---

# be-001 — Buat endpoint charge kartu
```

### `version` di brief dan PRD

`version` adalah bilangan bulat yang naik satu setiap kali isi dokumen berubah.

1. **Dokumen baru**: tulis `version: 1`.
2. **Mengubah dokumen yang sudah ada**: baca angka `version` saat ini, lalu tulis angka itu + 1. Kalau `version` belum ada, anggap 0 dan tulis `version: 1`.
3. Naikkan sekali per perubahan yang di-commit, bukan per kalimat yang diedit.
4. Web app menaikkan `version` otomatis setiap kali dokumen disimpan di sana. Perubahan lewat Git tidak dinaikkan otomatis, jadi kamu yang harus menaikkannya.

Contoh, mengubah brief yang berisi `version: 3`:

```markdown
---
version: 4
---

# Product Brief — MCAT Sesi 1
```

### `briefs` di PRD

`briefs` mencatat brief mana, dan versi berapa, yang sudah dicocokkan dengan isi PRD.

- Format: `br-NNN@<version>`, dipisah koma dan spasi, misalnya `briefs: br-001@3, br-002@1`.
- `br-NNN` diambil dari nama file brief: `briefs/br-002-sesi-2.md` menjadi `br-002`.
- `<version>` diambil dari `version` brief itu. Brief tanpa `version` dihitung 0.
- Bentuk lain (`[mcat-br-001]`, ID global, atau YAML list) tidak terbaca oleh Web app.

Cara mengisi saat menyusun atau memperbarui PRD:

1. Daftar semua file di `epics/<epic>/briefs/`.
2. Untuk setiap brief, baca `version`-nya.
3. Bandingkan dengan entri `briefs` di PRD:
   - brief tidak tercatat = brief baru, baca seluruhnya;
   - angka di brief lebih besar = brief berubah, cari bagian yang berubah;
   - tercatat tetapi file-nya sudah tidak ada = brief dihapus, sesuaikan PRD.
4. Perbarui isi PRD sampai sesuai dengan semua brief.
5. Tulis ulang `briefs` dengan versi yang baru saja dicocokkan, lalu naikkan `version` PRD.

Memperbaiki salah ketik di PRD tidak mengubah `briefs`, karena isinya tidak dicocokkan ulang dengan brief.

Contoh, PRD yang mencatat `br-001@2` padahal `br-001` sekarang `version: 3` dan ada brief baru `br-002` dengan `version: 1`:

```markdown
---
version: 6
briefs: br-001@3, br-002@1
---
```

## Izin per role

Role di Speq: `po`, `analyst`, `qa`, `ui_ux`, `engineer`, `project_admin`. Satu orang boleh memiliki beberapa role. `project_admin` memiliki semua izin role lain. Platform admin boleh menulis semua dokumen.

Izin bawaan (create dan update; delete dan restore mengikuti update):

| Role | Boleh menulis |
| --- | --- |
| semua role | `llms.txt` |
| `po` | `epics/*/briefs/**` |
| `analyst` | `README.md`, `epics/*/prd.md`, `epics/*/stories/*/README.md`, `epics/*/stories/*/acceptance-criteria.md`, `epics/*/decisions/**` |
| `qa` | `epics/*/stories/*/acceptance-criteria.md`, `epics/*/decisions/**` |
| `ui_ux` | `design/**`, `epics/*/stories/*/DESIGN.md`, `epics/*/stories/*/layout.html`, `epics/*/decisions/**` |
| `engineer` | `architecture.md`, `epics/*/architecture.md`, `epics/*/stories/*/tasks/**`, `epics/*/decisions/**` |

Akibatnya:

- `po` tidak dapat menulis PRD maupun Decision Record. Persetujuan PO dan keputusan milik PO dicatat oleh `analyst`.
- Hanya `engineer` yang dapat menulis task, kecuali project memberi izin tambahan.
- Satu project dapat menambah izin (`allow`) atau mencabutnya (`deny`). `deny` menang. Karena itu tabel di atas hanya bawaan; izin yang berlaku dibaca dari `speq list --json`.
- Brief terakhir di epic yang sudah memiliki PRD tidak dapat dihapus. Project yang diarsipkan tidak dapat diubah.

### Membaca izin yang berlaku

1. Di root repository specs: `git config --get speq.address`. Hasilnya `<api>/v1/projects/<project_id>`. Ambil `<project_id>`.
2. Jalankan `speq list --json`. Cari entri `projects` dengan `project_id` itu.
3. Ambil `roles` serta `capabilities.create` dan `capabilities.update`. Masing-masing berisi `allow` dan `deny`: daftar pola path.
4. Pola path: `*` cocok dengan tepat satu segmen, `**` di akhir cocok dengan satu atau lebih segmen. Contoh: `epics/*/stories/*/tasks/**` cocok dengan `epics/mcat/stories/st-001-login/tasks/be-001-api.md`.
5. File baru memakai `create`; file yang sudah ada memakai `update`. Sebuah path boleh ditulis bila cocok dengan salah satu pola `allow` dan tidak cocok dengan pola `deny` mana pun. Untuk platform admin, `allow` berisi `**`.

Bila `speq list` gagal karena belum login, minta user menjalankan `speq login`.

## Sumber di luar repository

Dokumen Speq harus dapat dipahami tanpa membuka lampiran. Perlakukan sumber dari luar repository specs menurut jenisnya:

| Jenis sumber | Contoh | Perlakuan |
| --- | --- | --- |
| Dokumen kebutuhan | Brief `.docx`, logic spec, notulen, email | Transformasikan isinya ke brief, PRD, story, dan AC. Tulis tautan sumber cukup satu baris di section References PRD |
| Data yang menentukan logika atau diuji QA | Band skor, daftar nama asesmen, aturan perhitungan, text bank yang dipakai sistem | Salin ke dokumen Speq sebagai tabel, di story yang memakainya (atau PRD bila lintas story). Tulis asal dan tanggal salinannya. Setelah itu Speq menjadi sumber kebenaran |
| Konten yang dikelola tim lain | Copy marketing, artikel, konten yang rutin diubah | Biarkan di sumbernya. Tulis tautan beserta versi atau tanggal yang dipakai untuk rilis ini |
| Desain visual | Figma | Tautan node Figma di `DESIGN.md`, beserta catatan state dan interaksi dalam teks |
| File biner | Gambar, PDF, template Canva | Tautan ke lokasinya; Speq hanya menyimpan dokumen teks |

Bila tidak jelas apakah sebuah data termasuk "menentukan logika" atau "konten tim lain", tanyakan ke user sebelum menyalinnya. Tautan antar dokumen Speq (PRD, story, AC, task) selalu boleh dipakai.

## `llms.txt`

`llms.txt` adalah indeks bersama, bukan salinan isi dokumen. Satu baris per dokumen, berisi tautan relatif dan ringkasan singkat, di section yang sesuai:

| Section | Isi |
| --- | --- |
| Product & requirements | `README.md`, PRD, story, acceptance criteria |
| Business briefs | Brief epic |
| Design | `design/DESIGN.md`, komponen, desain story |
| Architecture & implementation | Arsitektur produk/epic dan task |
| Decisions | Decision Record |

Contoh baris: `  - [mcat st-003 — Pengerjaan section sesi 1](epics/mcat/stories/st-003-pengerjaan-section/README.md): soal per section dengan timer.`

Tambahkan hanya dokumen yang benar-benar ada, dan perbarui tautan saat file dipindah atau diganti namanya.
