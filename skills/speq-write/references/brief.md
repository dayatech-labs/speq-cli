# Brief (PO)

Path: `epics/<epic>/briefs/br-NNN-<judul>.md`. Brief adalah permintaan bisnis dari sudut pandang PO. Brief menjadi masukan bagi Analyst, bukan instruksi implementasi.

Saat merapikan catatan mentah PO (notulen, chat, dokumen docx), pertahankan fakta dan istilah aslinya. Isi yang tidak ada di catatan ditulis sebagai pertanyaan di section *Open questions*, bukan dikarang. Kalau section wajib kosong, tulis `Belum ada informasi; ditanyakan ke <pihak>` dan tambahkan pertanyaannya.

## Kerangka

```markdown
---
version: 1
id: <epic>-br-NNN
type: brief
epic: <epic>
owner: PO
status: draft
requester: <nama>
urgency: <tingkat atau tenggat, sesuai catatan>
needed_by: YYYY-MM-DD
date: YYYY-MM-DD
source: <nama file atau tautan sumber, bila ada>
drafted_by: AI
updated: YYYY-MM-DD
---

# Product Brief — <judul>

**<Satu kalimat: masalah utama yang harus diselesaikan.>**

## Submission

- **Requester:** <nama dan tim>
- **Client:** <klien, atau "tidak khusus satu klien" beserta alasannya>
- **Urgency:** <tingkat urgensi>

## Background

<Konteks produk hari ini dan alasan permintaan ini dibahas sekarang.>

## Problem

<Satu masalah per butir. Sertakan bukti (data, keluhan, tiket) bila ada.>

## Affected roles

| Role | Yang mereka alami |
| --- | --- |
| | |

## User journey

<Alur per role, bernomor. Tulis yang terjadi hari ini dan yang diharapkan.>

## Impact if not solved

<Dampak bisnis atau operasional bila tidak dikerjakan.>

## Needed by

<Tanggal dan arti tanggal itu, misalnya acara klien atau rilis.>

## Definition of done

1. <Hasil yang dapat diamati yang menandai brief selesai.>

## Rules already decided by others

| Aturan | Diputuskan oleh |
| --- | --- |
| | |

## Out of scope

-

## Open questions

| # | Pertanyaan | Ditanyakan ke |
| --- | --- | --- |
| 1 | | |

## Specification

<Opsional: logic spec, tabel, istilah, atau tautan ke sumber eksternal.>
```
