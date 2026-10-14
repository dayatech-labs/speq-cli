# Architecture dan tech discovery (Engineer)

Path: `epics/<epic>/architecture.md` (arsitektur epic) atau `architecture.md` di root (lintas epic).

Ada dua tahap:

1. **Tech discovery**, segera setelah brief masuk. Engineer menjawab OQ teknis tentang perilaku sistem yang ada, kelayakan, dan batasan, supaya PRD tidak dibangun di atas asumsi. Analyst lalu memindahkan statusnya ke PRD.
2. **Arsitektur epic**, setelah handoff. Isinya komponen yang terdampak, kontrak API, data, keputusan, dan risiko.

## Aturan isi

- Setiap klaim tentang sistem yang ada menyebut buktinya: path file, endpoint, nama tabel, atau tautan dokumen. Klaim tanpa bukti ditulis `belum diverifikasi` dan dimasukkan ke daftar yang perlu dicek Engineer.
- Kalau sesi ini berjalan di repository implementasi, baca kodenya untuk menjawab OQ teknis. Jangan menebak stack, versi, atau struktur.
- Pilihan yang signifikan (database, layanan pihak ketiga, pola integrasi) ditulis singkat di tabel Keputusan, lalu diusulkan menjadi Decision Record (`decisions/dr-NNN-*.md`).
- Isi bisnis (aturan, angka) tidak ditulis ulang di sini. Rujuk FR/BR dengan ID-nya.

## Kerangka

```markdown
---
id: <epic>-architecture
type: architecture
epic: <epic>
owner: Engineer
status: draft
drafted_by: AI
updated: YYYY-MM-DD
---

# Architecture — <Nama epic>

## Tech discovery

Jawaban atas open question teknis di [PRD](prd.md#10-open-questions).

| OQ | Pertanyaan | Jawaban | Bukti | Tanggal |
| --- | --- | --- | --- | --- |
| OQ-NN | | <jawaban, atau "belum diverifikasi"> | <file, endpoint, dokumen> | |

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

<Tabel, kolom, event, atau file yang berubah; migrasi yang diperlukan.>

## Keputusan

| Keputusan | Alasan | Konsekuensi | DR |
| --- | --- | --- | --- |
| | | | |

## Risiko

- <Risiko, dampak, dan mitigasinya.>
```
