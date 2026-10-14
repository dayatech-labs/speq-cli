# PRD (Analyst)

Path: `epics/<epic>/prd.md`. PRD menjelaskan **mengapa** dan **apa** pada tingkat epic. FR, aturan khusus story, dan AC ditulis di folder story; PRD hanya menyebut rentang FR per story.

## Cara menyusun dari brief

1. Baca semua brief epic (`briefs/*.md`) dan section *Tech discovery* di `architecture.md` epic bila ada. Kalau PRD sudah ada, ubah bagian yang terdampak saja, dan pertahankan nomor ID lama.
   - Bandingkan `version` setiap brief dengan entri `briefs` di PRD. Brief yang versinya lebih baru adalah sumber perubahan. Brief yang belum tercatat adalah brief tambahan.
   - Setelah PRD diperbarui, isi `briefs` dengan versi brief yang dipakai, lalu naikkan `version` PRD.
2. Pecah cakupan menjadi story. Satu story mewakili satu hasil yang bernilai bagi satu role dan dapat diserahkan sendiri. Buat story hanya untuk cakupan brief yang sedang dikerjakan.
3. Turunkan open questions dari brief, lalu tambahkan celah yang Anda temukan. Tandai **blocking** bila story belum bisa dikerjakan sebelum pertanyaannya dijawab.
4. Setiap business rule, NFR, dan goal menyebut sumbernya (`brief: <section>`, `DoD <n>`). Turunan atau usulan Analyst yang tidak ada di brief ditandai *(Analyst)*.
5. Goal tanpa ukuran keberhasilan di brief: tulis Definition of done-nya, lalu jadikan ukuran setelah rilis sebuah OQ untuk PO.

## Kerangka

````markdown
---
version: 1
briefs: br-001@<version brief>
id: <epic>-prd
type: prd
epic: <epic>
owner: Analyst
status: draft
approved_by:
approved_at:
drafted_by: AI
updated: YYYY-MM-DD
---

# <Nama rilis> — <cakupan dalam satu frasa>

| | |
| --- | --- |
| **Module** | <area produk atau layar yang tersentuh> |
| **Product owner** | <nama PO> |
| **Written by** | Analyst |
| **Main users** | <role pengguna> |
| **Platform** | <aplikasi, bahasa, perangkat> |
| **Input** | [Product Brief](briefs/br-001-<judul>.md) |
| **Needed by** | <tanggal dan artinya, dari brief> |
| **Technical** | [architecture](architecture.md) · [indeks](../../llms.txt) |

## 1. Summary

<Kondisi hari ini, lalu yang dikerjakan rilis ini dalam daftar singkat per story.>

```mermaid
flowchart LR
    A["<bagian>"] --> B["<bagian>"]
```

<Satu paragraf: urutan pengerjaan dan alasannya.>

## 2. Problem

1. <Satu masalah per butir, dengan bukti dari brief.>

## 3. Goals & success metrics

| # | Goal | Problem | Success metric |
| --- | --- | --- | --- |
| G1 | | 1 | |

## 4. Users & access

| Role | What they do here |
| --- | --- |
| | |

## 5. Functional scope

Setiap story merinci FR, aturan, dan acceptance criteria di folder story. Nomor FR unik di seluruh epic.

### 5.1 <Nama story> (st-001)

Sebagai <role>, saya ingin <kemampuan>, supaya <manfaat>. (G1 · FR-01–FR-NN)

Rincian: [kebutuhan dan aturan](stories/st-001-<nama>/README.md) · [acceptance criteria](stories/st-001-<nama>/acceptance-criteria.md) · [tasks](stories/st-001-<nama>/tasks/)

### 5.N Status story

| Story | FR/NFR | Perlu desain | Blocked by | Status |
| --- | --- | --- | --- | --- |
| st-001 | | | | draft |

## 6. Business rules

- **BR-01** <aturan lintas story>. *(st-001, st-002 · brief: <section>)*

## 7. Non-functional requirements

| ID | Requirement | Story |
| --- | --- | --- |
| NFR-01 | <dengan angka yang terukur, atau "lihat OQ-NN"> | |

## 8. Out of scope

-

## 9. Dependencies

| Dependensi | Dibutuhkan oleh | Status |
| --- | --- | --- |
| | | |

## 10. Open questions

| ID | Pertanyaan | Owner | Blocking | Status |
| --- | --- | --- | --- | --- |
| OQ-01 | | | st-001 | Open |

### By when they must be answered

| When | Questions |
| --- | --- |
| | |

## 11. References

| Sumber | Dipakai untuk | Status |
| --- | --- | --- |
| | | |

## 12. Glossary

| Term | Meaning |
| --- | --- |
| | |
````
