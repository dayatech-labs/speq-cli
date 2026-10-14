# Memindahkan dokumentasi lama ke Speq

Untuk dokumentasi yang sudah ada di repository implementasi (`docs/`, `README.md`, ADR, wiki yang diekspor) dan perlu dipindah ke repository specs (`./specs`).

Pemindahan **bukan salin per file**. Satu spesifikasi fitur lama bisa menjadi PRD, README story, dan acceptance criteria. Beberapa dokumen arsitektur bisa digabung menjadi satu `architecture.md`. Dokumentasi engineering tetap di repository asal.

## Langkah 1: inventaris (belum menulis apa pun)

Baca semua dokumen sumber, lalu tampilkan tabel ini ke user dan tunggu persetujuannya:

| Sumber | Tujuan di Speq | Catatan |
| --- | --- | --- |
| `docs/payment.md` | `epics/payment/prd.md`, `stories/st-001-.../README.md`, `acceptance-criteria.md` | Bagian "Rules" menjadi BR |
| `docs/adr/0003-queue.md` | `epics/payment/decisions/dr-001-use-queue.md` | |
| `docs/setup.md` | — (tetap di repository asal) | Panduan setup dev |

Pemetaan default:

| Isi dokumen sumber | Tujuan |
| --- | --- |
| Gambaran produk, tujuan, pengguna | `README.md` dan `llms.txt` |
| Permintaan bisnis, latar belakang, masalah | `epics/<epic>/briefs/br-NNN-*.md` |
| Requirement, spesifikasi fitur, user story | `prd.md`, `stories/st-NNN-*/README.md` |
| Kriteria penerimaan, skenario uji | `stories/st-NNN-*/acceptance-criteria.md` |
| ADR, catatan keputusan | `epics/<epic>/decisions/dr-NNN-*.md` |
| Arsitektur, diagram sistem, kontrak API, model data | `architecture.md` (produk) atau `epics/<epic>/architecture.md` |
| Design system, panduan UI | `design/DESIGN.md` |
| Rencana kerja per fitur | `stories/st-NNN-*/tasks/<jenis>-NNN-*.md` |
| Setup dev, runbook, deploy, changelog, troubleshooting | Tetap di repository asal |

Kelompokkan isi ke epic, yaitu area produk jangka panjang. Usulkan nama epic di tabel dan biarkan user memutuskan. Kalau user memberi aturan khusus untuk repository ini (misalnya "folder `docs/rfc/` adalah decision record"), aturan user menang.

## Langkah 2: konversi per epic

- Susun ulang isi ke kerangka di `references/`, dengan ID dan traceability sesuai `conventions.md`. Jangan menempel teks lama apa adanya bila tidak sesuai bentuk dokumen tujuan.
- Catat asal setiap dokumen di frontmatter: `source: docs/payment.md` (beberapa sumber dipisah koma).
- Informasi yang usang, saling bertentangan, atau tidak jelas ditulis sebagai `OQ-NN` dan tidak boleh dipilih sendiri. Sebutkan kedua versi di pertanyaannya.
- Gambar tidak dapat dipindah karena Speq hanya menerima dokumen teks. Tulis tautan ke lokasi gambar di repository asal, lalu catat di balasan.
- Klaim tentang perilaku sistem dicocokkan dengan kode bila kode tersedia. Klaim yang berbeda dengan kode ditandai `belum diverifikasi`.
- Kerjakan satu epic sampai selesai, jalankan checklist **speq-review**, baru lanjut ke epic berikutnya.

## Langkah 3: push dan sumber lama

- Dari `./specs`: `speq add`, `speq commit -m "docs: import <epic> from <repo>"`, `speq push`. Satu push paling banyak 20 commit. Bila user bukan `project_admin`, kelompokkan commit per role.
- **Jangan menghapus atau mengubah dokumen sumber.** Setelah push berhasil, tawarkan dua pilihan kepada user: menghapus dokumen yang sudah dipindah, atau menggantinya dengan satu baris tautan ke dokumen Speq. Perubahan di repository asal di-commit dengan `git`, bukan `speq`.
- Balas dengan tabel akhir sumber → dokumen Speq, OQ yang ditambahkan, dan isi yang tidak dipindah beserta alasannya.
