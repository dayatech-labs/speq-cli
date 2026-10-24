# Story (analyst)

Path: `epics/<epic>/stories/st-NNN-<nama>/README.md`. Buat satu folder per story yang tercantum di PRD §5. Story tidak memakai frontmatter, kecuali `ticket-id` setelah story punya tiket Dayaboard (`conventions.md` §Frontmatter).

Kerangka di bawah adalah bentuk yang dipakai tim sejak 24 Okt 2026, supaya isi story dan deskripsi tiket Dayaboard sama urutannya. Section yang tidak berlaku untuk satu story ditulis `—`, bukan dihapus: nomor section tetap sama di semua story.

**Story yang ditulis sebelum 24 Okt 2026** memakai kerangka tujuh section yang lama. Jangan memindahkan semuanya sekaligus: satu story pindah ke kerangka ini saat ia memang sedang diedit, sekali jalan dan utuh, bukan section baru ditempel di atas yang lama. Nomor section bergeser, jadi perbaiki juga rujukan dari `acceptance-criteria.md` dan `tasks/` story yang sama — `§4 aturan 2` menjadi `§9 aturan 2`, `README §6` menjadi `README §11`. Sebutkan perpindahan itu di laporan, supaya pemilik story tahu nomor rujukannya berubah.

## Aturan isi

- **Acuan desain** di bawah judul: satu tautan ke layar yang dirancang, beserta versi atau tanggalnya. Figma memakai tautan node; prototype memakai tautan layar atau berkas tangkapan layar di repo desain. Bila belum ada desain, tulis `**Acuan desain**: belum ada`.
- **Perilaku** (section 7) adalah daftar bernomor (1, 2, 3, ...) tentang apa yang dilakukan sistem untuk pengguna. Satu butir berisi satu perilaku yang dapat diamati, dengan sumbernya: `*(brief: <section>, DoD <n>)*`. Butir yang berisi dua perilaku dipecah menjadi dua. Jangan memberi ID seperti FR-01.
- **Alur proses** (section 6) memakai penomoran langkah per alur. Alur alternatif diberi ID `A1`, `A2`, ...; alur kegagalan diberi ID `E1`, `E2`, .... AC merujuk ID itu, jadi nomornya tidak pernah dipakai ulang di story yang sama.
- **Aturan** (section 9) khusus story ditulis sebagai daftar bernomor dan dirujuk dokumen lain sebagai `§9 aturan 2`; jangan memberinya ID `BR-NN`, yang nomornya milik PRD §6. Aturan lintas story dirujuk ke PRD dengan kode aslinya. Tabel di bawah memuat keduanya: kolom Kode berisi `Aturan 1`, `Aturan 2`, ... untuk aturan story, dan `BR-NN` untuk aturan PRD yang berlaku di story ini.
- **Data yang menentukan perilaku** (daftar kolom, pemetaan database, kriteria pencarian, isi notifikasi, perubahan data per aksi) disalin sebagai tabel di section 8, beserta asal dan tanggal salinannya. Lihat `conventions.md` §Sumber di luar repository.
- Perilaku yang belum diputuskan ditulis apa adanya dengan `menunggu: <ringkasan pertanyaan> (PRD §10)`, bukan diisi tebakan.
- **Tanda `[asumsi]`.** Kalimat yang belum dikonfirmasi pemilik keputusan diberi tanda `*[asumsi]*` di akhir kalimat, **dan** wajib punya pertanyaan terbuka yang sama di section 11. Tanda tanpa pertanyaan adalah tebakan terselubung; pertanyaan tanpa tanda membuat pembaca tiket tidak tahu bagian mana yang belum pasti.
- Section 10 menyebut apakah story butuh desain, dan layar atau state mana yang perlu didesain.
- Acceptance criteria tetap di `acceptance-criteria.md`, dan technical task tetap di `tasks/`. Story tidak menyalin keduanya; skill `speq-dayaboard` yang menggabungkannya saat menulis deskripsi tiket.

## Kerangka

```markdown
# st-NNN — <Nama story>

**Acuan desain**: [<nama layar>](<tautan node atau berkas>) · <versi atau tanggal>

| | |
| --- | --- |
| **Pengguna** | <role pengguna> |
| **Goal** | G<n> |
| **Prasyarat** | <story lain atau dependensi, atau —> |
| **Dokumen** | [acceptance criteria](acceptance-criteria.md) · [tasks](tasks/) · [PRD](../../prd.md) |

## 1. Latar belakang

<Masalah yang diselesaikan story ini dan posisinya di alur epic, dua sampai empat kalimat.>

## 2. User story

**Sebagai** <role pengguna>, **saya ingin** <kemampuan>, **agar** <manfaat>.

## 3. Ruang lingkup

**Termasuk:**

- <Yang dikerjakan di story ini.>

**Tidak termasuk:**

- <Yang sengaja tidak dikerjakan, dengan rujukan story lain bila ada.>

## 4. Aktor dan hak akses

| Aktor | Hak akses |
| --- | --- |
| <role> | <aksi yang boleh dilakukan> |
| Role tanpa permission | <apa yang dilihat atau ditolak> |

## 5. Pre-condition

- <Keadaan yang harus benar sebelum alur berjalan: login, permission, data master, perubahan struktur tabel.>

## 6. Alur proses

**Alur utama — <nama>:**

1. <Langkah pengguna dan respons sistem.>

**Alur — <aksi>:**

1. <Langkah pengguna dan respons sistem.>

**Alur alternatif:**

- A1: <kondisi> → <yang ditampilkan sistem>.

**Alur kegagalan:**

- E1: <kegagalan> → <pesan dan keadaan data sesudahnya>.

## 7. Perilaku

1. <Perilaku yang dapat diamati>. *(brief: <section>, DoD <n>)*

## 8. Data dan pemetaan

<Tabel data yang menentukan perilaku. Pakai sub-heading sesuai kebutuhan story,
misal "Kolom tabel dan pemetaan database", "Kriteria pencarian", "Isi notifikasi",
"Perubahan data per aksi". Sebutkan sumber dan tanggal salinannya. Tulis "—" bila
story tidak punya data seperti ini.>

## 9. Aturan

| Kode | Aturan |
| --- | --- |
| Aturan 1 | <Aturan khusus story ini.> |
| BR-NN | <Aturan PRD §6 yang berlaku di story ini.> |

## 10. Desain

<"Perlu desain: <layar dan state>." atau "Tidak ada perubahan UI: <alasan>.">
<Bila desain sudah ada: layar mana yang berlaku, dan apa yang harus dikonfirmasi
ke analyst saat story dan desain berbeda.>

## 11. Pertanyaan terbuka dan dependensi

- [ ] <Pertanyaan yang hanya menyangkut story ini> (ditanyakan ke: <role>)
- Pertanyaan lintas story: lihat PRD §10.
- Dependensi: <data, layanan, story lain, atau task yang memblokir>.
```
