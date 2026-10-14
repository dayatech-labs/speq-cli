# Produk baru

Dua bagian: menyiapkan repository dan project (manual, oleh platform admin), lalu menulis kerangka dokumen awal.

## 1. Persiapan oleh platform admin

Speq tidak membuat repository GitHub. API hanya memeriksa dan mengikat (bind) repository yang sudah ada. Tunjukkan checklist ini ke user dan tunggu sampai semua langkah selesai.

1. Buat repository GitHub `<org>/<produk>-specs`. Repository harus private, harus memiliki branch `main` dengan minimal satu commit, dan nama harus berakhiran `-specs`.

   ```bash
   gh repo create <org>/<produk>-specs --private --add-readme
   ```

2. Pastikan Speq GitHub App memiliki akses ke repository itu: GitHub → Settings organisasi → GitHub Apps → Speq → Repository access. Langkah ini tidak diperlukan bila app dipasang untuk semua repository.
3. Di Web app Speq, halaman `/platform`:
   1. Buat project dengan nama produk.
   2. Bind project ke `<org>/<produk>-specs`. Lakukan ini **sebelum** ada commit lewat Speq, karena binding terkunci setelah project memiliki commit.
   3. Tambahkan member beserta role-nya (PO, Analyst, QA, UIUX, Engineer).
4. Di repository implementasi atau folder kerja: `speq list`, lalu `speq clone <produk>-specs`.

Arti error saat bind:

| Kode | Penyebab | Perbaikan |
| --- | --- | --- |
| `repository_name_invalid` | Nama tidak berakhiran `-specs` | Ganti nama repository |
| `repository_not_private` | Repository public | Ubah menjadi private |
| `app_not_installed` | Speq GitHub App belum dipasang | Pasang app di organisasi |
| `repository_not_accessible` | App terpasang, tetapi tidak diberi akses ke repository ini | Tambahkan repository di Repository access |
| `branch_not_found` | Belum ada branch `main` | Buat commit awal di `main` |
| `repository_already_bound` | Repository sudah dipakai project lain | Pakai repository lain |

## 2. Kerangka dokumen awal

Mulai setelah `speq clone` berhasil. Minta deskripsi produk ke user bila belum ada: nama, masalah yang diselesaikan, pengguna, platform, dan epic yang sudah diketahui. Jangan mengarang informasi itu.

| File | Isi | Status |
| --- | --- | --- |
| `README.md` | Gambaran produk dalam satu paragraf, tabel epic (Epic, Cakupan, Brief aktif, Status PRD), struktur folder, ringkasan konvensi ID dan status, tabel siapa mengubah apa. Rujuk `conventions.md` | `draft` |
| `llms.txt` | Judul produk, ringkasan satu kalimat dalam blockquote, lalu section per role sesuai `conventions.md`. Isi hanya dengan dokumen yang sudah ada | — |
| `architecture.md` | Kerangka `references/architecture.md` untuk tingkat produk: Konteks, Komponen, Data, Keputusan, Risiko | `template` |
| `design/DESIGN.md` | Kerangka aturan desain: prinsip, warna, tipografi, komponen bersama, aksesibilitas | `template` |
| `epics/<epic>/briefs/br-001-<judul>.md` | Hanya bila user memberi catatan brief; ikuti `references/brief.md` | `draft` |

`README.md` dari `gh repo create --add-readme` diganti seluruhnya.

Push:

- Buat satu commit `docs: set up <produk> specs`.
- `project_admin` dapat push semua file sekaligus. Bila user bukan admin, kelompokkan commit per role sesuai izin di `conventions.md`, dan beri tahu user file mana yang harus di-push role lain.
