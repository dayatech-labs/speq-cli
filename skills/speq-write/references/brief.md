# Brief (po)

Path: `epics/<epic>/briefs/br-NNN-<judul>.md`. Brief adalah permintaan bisnis dari sudut pandang PO. Brief menjadi masukan bagi Analyst, bukan instruksi implementasi.

Saat merapikan catatan mentah PO (notulen, chat, `.docx`), pertahankan fakta dan istilah aslinya. Isi yang tidak ada di catatan ditulis sebagai butir di *Open questions*, bukan dikarang. Section wajib yang kosong diisi `Belum ada informasi.` dan pertanyaannya ditambahkan.

Frontmatter hanya berisi `version` (lihat `conventions.md` §Frontmatter): `version: 1` untuk brief baru, angka lama + 1 untuk brief yang diubah.

## Kerangka

```markdown
---
version: 1
---

# Product Brief — <judul>

**<Satu kalimat: masalah utama yang harus diselesaikan.>**

| | |
| --- | --- |
| **Requester** | <nama dan tim> |
| **Client** | <klien, atau "tidak khusus satu klien" beserta alasannya> |
| **Urgency** | <tingkat urgensi> |
| **Needed by** | <tanggal dan arti tanggal itu> |
| **Date** | <tanggal brief diajukan> |

## Background

<Konteks produk hari ini dan alasan permintaan ini dibahas sekarang.>

## Problem

<Satu masalah per butir. Sertakan bukti (data, keluhan, tiket) bila ada.>

## Affected roles

| Role pengguna | Yang mereka alami |
| --- | --- |
| | |

## User journey

<Alur per role pengguna, bernomor: yang terjadi hari ini dan yang diharapkan.>

## Impact if not solved

<Dampak bisnis atau operasional bila tidak dikerjakan.>

## Definition of done

1. <Hasil yang dapat diamati yang menandai brief selesai.>

## Rules already decided by others

| Aturan | Diputuskan oleh |
| --- | --- |
| | |

## Out of scope

-

## Open questions

- [ ] <Pertanyaan> (ditanyakan ke: <pihak>)

## Specification

<Opsional: logic spec, tabel, atau istilah. Data yang menentukan logika disalin sebagai tabel di sini atau di story, bukan dilampirkan; lihat `conventions.md` §Sumber di luar repository.>
```
