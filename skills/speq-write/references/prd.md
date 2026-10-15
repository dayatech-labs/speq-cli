# PRD (analyst)

Path: `epics/<epic>/prd.md`. PRD menjelaskan **mengapa** dan **apa** pada tingkat epic. Perilaku, aturan khusus story, dan AC ditulis di folder story.

## Cara menyusun dari brief

1. Baca semua brief epic (`briefs/*.md`) dan section *Tech discovery* di `architecture.md` epic bila ada. Kalau PRD sudah ada, ikuti langkah `briefs` di `conventions.md` §Frontmatter untuk menemukan brief yang baru atau berubah, lalu ubah bagian yang terdampak saja. Pertahankan nomor `st-NNN` dan `BR-NN` lama.
2. Pecah cakupan menjadi story. Satu story mewakili satu hasil yang bernilai bagi satu jenis pengguna dan dapat diserahkan sendiri. Buat story hanya untuk cakupan brief yang sedang dikerjakan.
3. Aturan yang berlaku untuk lebih dari satu story menjadi `BR-NN` di §6. Aturan satu story ditulis di README story itu.
4. Syarat umum (bahasa, browser, perangkat, performa, keamanan, SEO, analytics) ditulis di §7 sebagai daftar biasa, dengan angka yang terukur.
5. Salin open questions dari brief ke §10, lalu tambahkan celah yang kamu temukan. Tandai **blocking** bila story belum dapat dikerjakan engineering sebelum dijawab.
6. Setiap goal, BR, dan kebutuhan kualitas menyebut sumbernya (`brief: <section>`, `DoD <n>`). Turunan atau usulan Analyst yang tidak ada di brief ditandai *(Analyst)*.
7. Goal tanpa ukuran keberhasilan di brief: tulis Definition of done-nya, lalu tambahkan pertanyaan ke PO tentang ukuran setelah rilis.
8. Isi frontmatter: `version` (angka lama + 1, atau 1 untuk PRD baru) dan `briefs` dengan versi setiap brief yang sudah dicocokkan.

Persetujuan PO dicatat Analyst di baris **Approved by PO**, karena role `po` tidak dapat menulis PRD.

## Kerangka

````markdown
---
version: 1
briefs: br-001@1
---

# <Nama rilis> — <cakupan dalam satu frasa>

| | |
| --- | --- |
| **Module** | <area produk atau layar yang tersentuh> |
| **Product owner** | <nama PO> |
| **Approved by PO** | <nama, tanggal; atau "belum"> |
| **Main users** | <role pengguna> |
| **Platform** | <aplikasi, bahasa, perangkat> |
| **Input** | [Brief br-001](briefs/br-001-<judul>.md) |
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

| Role pengguna | What they do here |
| --- | --- |
| | |

## 5. Stories

### 5.1 <Nama story> (st-001)

Sebagai <role>, saya ingin <kemampuan>, supaya <manfaat>. (G1)

Rincian: [story](stories/st-001-<nama>/README.md) · [acceptance criteria](stories/st-001-<nama>/acceptance-criteria.md) · [tasks](stories/st-001-<nama>/tasks/)

### 5.N Ringkasan story

| Story | Perlu desain | Menunggu pertanyaan |
| --- | --- | --- |
| st-001 | ya / tidak | <ringkasan pertanyaan blocking, atau —> |

## 6. Business rules

- **BR-01** <aturan lintas story>. *(st-001, st-002 · brief: <section>)*

## 7. Kebutuhan kualitas

1. <Syarat umum dengan angka terukur, misalnya "Halaman tersedia dalam ID dan EN">. *(brief: <section>)*

## 8. Out of scope

-

## 9. Dependencies

| Dependensi | Dibutuhkan oleh | Status |
| --- | --- | --- |
| | | |

## 10. Pertanyaan terbuka

- [ ] <Pertanyaan> (ditanyakan ke: <role> · **blocking** st-NNN · dijawab sebelum <tanggal>)

## 11. References

| Sumber | Dipakai untuk |
| --- | --- |
| <tautan sumber di luar repository, satu baris per sumber> | <bagian PRD yang memakainya> |

## 12. Glossary

| Term | Meaning |
| --- | --- |
| | |
````
