# Task (Engineer)

Path: `epics/<epic>/stories/st-NNN-<nama>/tasks/<jenis>-NNN-<judul>.md`, satu file per task. Task menjelaskan **cara** menyelesaikan kebutuhan yang sudah disetujui.

## Sebelum memecah task

1. Baca README story, `acceptance-criteria.md`, `DESIGN.md` bila ada, `architecture.md` epic dan produk, serta task yang sudah ada di folder `tasks/`.
2. Kalau sesi ini berjalan di repository implementasi (ada kode di luar folder specs), baca modul yang akan disentuh. Sebutkan file atau endpoint nyata. Jangan menebak nama file.
3. Daftar AC yang belum ditutup task mana pun adalah target pemecahan.

## Aturan granularitas

- **Satu task, satu PR.** Task dapat direview sendiri, selesai dalam sekitar dua hari kerja, dan berada di satu lapisan: `be` (backend), `fe` (frontend), `qa` (data uji dan verifikasi), atau jenis lain yang dipakai tim.
- **Terhubung ke AC.** Setiap task menyebut AC yang diselesaikannya, dan setiap AC story ditutup minimal satu task.
- **Tidak menambah aturan bisnis.** Bila task membutuhkan aturan yang belum ada (batas retry, format, pesan error), catat pertanyaannya di section 6 README story dan kembalikan ke Analyst.
- **Menunggu OQ.** Task yang bergantung pada OQ blocking mencantumkannya di `waiting_on` dan berstatus `blocked`.
- **Dependensi.** Dalam story yang sama cukup ID singkat (`be-001`). Rujukan ke story lain memakai ID global (`mcat-st-001-be-001`).
- **Draf.** Task yang ditulis Analyst atau AI berstatus `draft` dengan `drafted_by`. Engineer memvalidasi, memecah ulang, dan memberi estimasi, lalu mengubah status menjadi `todo`.
- **Tiket.** Judul tiket di project tracker memakai format `[<ID task global>] <judul task>`.

## Kerangka

```markdown
---
id: <epic>-st-NNN-be-001
type: task
epic: <epic>
story: <epic>-st-NNN
owner: Engineer
drafted_by: AI
status: draft
ac: [AC-01, AC-02]
depends_on: []
waiting_on: []
estimate:
updated: YYYY-MM-DD
---

# <epic>-st-NNN-be-001 — <judul task, kata kerja di depan>

| | |
| --- | --- |
| **Jenis** | BE |
| **AC** | [AC-01, AC-02](../acceptance-criteria.md) |
| **Bergantung pada** | — |
| **Menunggu** | — |

## Rincian

<Perilaku yang dibangun, dalam urutan pengerjaan. Rujuk FR/BR dengan ID-nya.>

## Catatan teknis

| | |
| --- | --- |
| **Komponen** | <aplikasi, modul, route, atau file nyata> |
| **Kontrak API** | <method + path + payload ringkas, atau tautan ke architecture.md> |
| **Data** | <tabel/kolom/event yang berubah, atau "tidak ada"> |

Aturan yang ditegakkan di task ini:

| Aturan | Ditegakkan di | Perilaku bila dilanggar |
| --- | --- | --- |
| FR-NN — <ringkasan> | <lapisan atau modul> | <respons, pesan, AC terkait> |

## Selesai bila

- <Kondisi yang dapat diperiksa reviewer atau QA, misalnya "AC-01 lolos di staging" atau "endpoint mengembalikan 409 untuk ...".>
```
