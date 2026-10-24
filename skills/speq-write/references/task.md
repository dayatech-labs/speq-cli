# Task (engineer)

Path: `epics/<epic>/stories/st-NNN-<nama>/tasks/<jenis>-NNN-<judul>.md`, satu file per task. Task menjelaskan **cara** menyelesaikan story yang sudah ada. Frontmatter task berisi `status` dan, setelah task punya tiket Dayaboard, `ticket-id` (`conventions.md` §Frontmatter).

Secara bawaan hanya role `engineer` yang dapat push task. Bila user bukan engineer, tanyakan dulu apakah ia ingin draf lokal untuk diserahkan ke engineer.

## Sebelum memecah task

1. Jalankan `speq context <epic>/<story>` dan baca setiap file yang ditampilkan: README story, `acceptance-criteria.md`, `DESIGN.md`, `layout.html`, kedua `architecture.md`, serta task yang sudah ada.
2. Kalau sesi ini berjalan di repository implementasi (ada kode di luar folder specs), baca modul yang akan disentuh. Sebutkan file atau endpoint nyata. Jangan menebak nama file.
3. Daftar AC yang belum ditutup task mana pun adalah target pemecahan.

## Aturan granularitas

- **Satu task, satu PR.** Task dapat direview sendiri, selesai dalam sekitar dua hari kerja, dan berada di satu lapisan: `be`, `fe`, `qa`, atau jenis lain yang dipakai tim.
- **Terhubung ke AC.** Setiap task menyebut AC yang diselesaikannya, dan setiap AC story ditutup minimal oleh satu task.
- **Tidak menambah aturan bisnis.** Bila task membutuhkan aturan yang belum ada (batas retry, format, pesan error), tambahkan pertanyaannya ke README story section 11 dan kembalikan ke analyst.
- **Menunggu jawaban.** Task yang bergantung pada pertanyaan blocking menuliskannya di baris **Menunggu**.
- **Dependensi.** Dalam story yang sama cukup ID singkat (`be-001`). Rujukan ke story lain memakai story dan ID-nya (`mcat st-001 be-001`).
- **Tiket.** Judul tiket di project tracker memakai format `[<epic> <story> <ID task>] <judul task>`, misalnya `[mcat st-003 be-001] Pembentukan section`.

## Status task

Setiap task punya `status` di frontmatter: `TODO` (task baru), `IN-PROGRESS`, `BLOCKED`, atau `DONE`, huruf besar semua (`conventions.md` §Frontmatter). Ubah sesuai keadaan sebenarnya. `DONE` hanya ditulis setelah task diimplementasi (kode ada dan syarat *Selesai bila* terpenuhi, atau user menyatakannya selesai). Untuk `BLOCKED`, tulis penyebabnya di baris **Menunggu**.

## Kerangka

```markdown
---
status: TODO
---

# be-001 — <judul task, kata kerja di depan>

| | |
| --- | --- |
| **Story** | [st-NNN <nama story>](../README.md) |
| **Jenis** | BE |
| **AC** | [AC-01, AC-02](../acceptance-criteria.md) |
| **Bergantung pada** | — |
| **Menunggu** | <ringkasan pertanyaan blocking, atau —> |
| **Estimasi** | <hari kerja, diisi engineer> |

## Rincian

<Perilaku yang dibangun, dalam urutan pengerjaan. Rujuk perilaku story dan BR yang ditegakkan.>

## Catatan teknis

| | |
| --- | --- |
| **Komponen** | <aplikasi, modul, route, atau file nyata> |
| **Kontrak API** | <method + path + payload ringkas, atau tautan ke architecture.md> |
| **Data** | <tabel/kolom/event yang berubah, atau "tidak ada"; nama tabel memakai prefix `master_` atau `trans_`; tabel baru memuat enam kolom audit di migrasi yang sama, lihat `conventions.md` §Kolom audit tabel database> |

Aturan yang ditegakkan di task ini:

| Aturan | Ditegakkan di | Perilaku bila dilanggar |
| --- | --- | --- |
| <Perilaku n atau BR-NN> | <lapisan atau modul> | <respons, pesan, AC terkait> |

## Selesai bila

- <Kondisi yang dapat diperiksa reviewer atau QA, misalnya "AC-01 lolos di staging" atau "endpoint mengembalikan 409 untuk ...".>
```
