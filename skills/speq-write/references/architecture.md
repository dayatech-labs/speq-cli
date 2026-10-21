# Architecture dan tech discovery (engineer)

Path: `epics/<epic>/architecture.md` (arsitektur epic) atau `architecture.md` di root (lintas epic). Tidak memakai frontmatter. Hanya role `engineer` yang dapat push file ini.

Ada dua tahap:

1. **Tech discovery**, segera setelah brief masuk. Engineer menjawab pertanyaan teknis tentang perilaku sistem yang ada, kelayakan, dan batasan, supaya PRD tidak dibangun di atas asumsi. Analyst lalu memindahkan jawabannya ke PRD dan menghapus pertanyaannya dari PRD §10.
2. **Arsitektur epic**, sebelum task dipecah. Isinya komponen yang terdampak, kontrak API, data, keputusan, dan risiko.

## Aturan isi

- Setiap klaim tentang sistem yang ada menyebut buktinya: path file, endpoint, nama tabel, atau tautan dokumen. Klaim tanpa bukti ditulis `belum diverifikasi` dan dimasukkan ke daftar yang perlu dicek engineer.
- Kalau sesi ini berjalan di repository implementasi, baca kodenya untuk menjawab pertanyaan teknis. Jangan menebak stack, versi, atau struktur.
- Pilihan yang signifikan (database, layanan pihak ketiga, pola integrasi) ditulis singkat di tabel Keputusan, lalu diusulkan menjadi Decision Record (`decisions/dr-NNN-*.md`).
- Tabel baru memakai prefix `master_` atau `trans_` (`conventions.md` §Nama tabel database). Entitas baru yang belum terdaftar di pengelompokan prefix produk ditambahkan ke sana.
- Isi bisnis (aturan, angka) tidak ditulis ulang di sini. Rujuk perilaku story atau `BR-NN`.

## Kerangka

```markdown
# Architecture — <Nama epic>

## Tech discovery

Jawaban atas pertanyaan teknis di [PRD §10](prd.md#10-pertanyaan-terbuka).

| Pertanyaan | Jawaban | Bukti | Tanggal |
| --- | --- | --- | --- |
| <salinan pertanyaan dari PRD> | <jawaban, atau "belum diverifikasi"> | <file, endpoint, dokumen> | |

Catatan sistem yang ada dan relevan untuk epic ini:

-

## Konteks

<Stack, hosting, build/deploy, dan batas sistem yang disentuh epic ini.>

## Komponen terdampak

| Komponen | Perubahan | Story |
| --- | --- | --- |
| | | st-NNN |

## Kontrak API

| Method | Path | Request | Response | Story |
| --- | --- | --- | --- | --- |
| | | | | |

## Data

<Tabel, kolom, event, atau file yang berubah; migrasi yang diperlukan. Nama tabel mengikuti `conventions.md` §Nama tabel database (`master_` atau `trans_`). Setiap tabel baru memuat keenam kolom audit di migrasi yang sama (`conventions.md` §Kolom audit tabel database).>

## Keputusan

| Keputusan | Alasan | Konsekuensi | DR |
| --- | --- | --- | --- |
| | | | |

## Risiko

- <Risiko, dampak, dan mitigasinya.>
```
