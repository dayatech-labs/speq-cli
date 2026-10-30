# Review dokumen dan kesiapan handoff

Dipakai dalam tiga keadaan. Review menghasilkan laporan temuan dan tidak mengubah keputusan pemilik dokumen.

| Keadaan | Contoh | Hasil |
| --- | --- | --- |
| Dokumen yang baru kamu tulis | Langkah 4 di `SKILL.md` | Perbaiki langsung, tanpa laporan |
| Dokumen yang sudah ada | "Cek AC st-002 ini", "review PRD mcat" | Laporan temuan. Edit hanya bila user minta **dan** izinnya mengizinkan path itu (`conventions.md` §Izin per role). Bila path milik role lain, tulis temuannya sebagai pertanyaan untuk pemilik dokumen |
| Kesiapan handoff | "Apakah st-003 siap untuk engineering?", "cek epic mcat sebelum handoff" | Daftar periksa lintas dokumen di bawah, dengan jawaban siap atau belum per story |

## Daftar periksa per dokumen

**Semua dokumen**

- Path dan nama file sesuai `conventions.md`.
- Frontmatter hanya memakai key yang diizinkan `conventions.md` §Frontmatter: `version` (brief, PRD), `briefs` (PRD), `ticket-id` (PRD, README story, task), `status` (task: `TODO`, `IN-PROGRESS`, `BLOCKED`, `DONE`, huruf besar; `DONE` hanya bila task sudah diimplementasi), `last-update` (`development-status.md`). Key lain dihapus; `ticket-id` yang sudah ada dipertahankan.
- Tautan relatif mengarah ke file yang ada.
- Setiap aturan, angka, nama, dan tanggal punya sumber (brief, PRD, BR, atau pertanyaan terbuka). Yang tidak bersumber dan belum diputuskan menjadi pertanyaan.
- Tidak ada sisa template: `<...>`, `TBD`, `TODO`, atau komentar HTML.
- Tidak ada ID selain `st-NNN`, `AC-NN`, `BR-NN`, dan ID task. FR, NFR, dan OQ bernomor diganti sesuai `conventions.md`.

**Per jenis**

| Dokumen | Periksa |
| --- | --- |
| Brief | `version` ada. Problem disertai bukti. Definition of done dapat diamati. Open questions menyebut pihak yang ditanya |
| PRD | `version` ada. `briefs` memuat setiap brief epic dengan format `br-NNN@<version>`, dan setiap versi sama dengan `version` brief saat ini (bila tidak, PRD tertinggal; sebutkan brief-nya). Problem bernomor, goal merujuk problem, metrik terukur atau menjadi pertanyaan. Ringkasan story ada. Pertanyaan blocking menyebut story-nya |
| Story | Kesebelas section ada dan berurutan (yang tidak berlaku ditulis "—"). Acuan desain tertulis atau dinyatakan belum ada. User story berisi role, kemampuan, dan manfaat. Setiap butir perilaku adalah satu perilaku yang dapat diamati, dengan sumber. Ruang lingkup memuat yang termasuk dan tidak termasuk. Setiap alur alternatif dan kegagalan punya ID A/E. Setiap tanda `[asumsi]` punya pertanyaan terbuka yang sama di section 11 |
| Acceptance criteria | Given/When/Then. Setiap *Then* dapat diamati. Setiap butir perilaku story diuji minimal oleh satu AC. Ada kasus gagal. Tidak menyebut implementasi |
| Task | Menyebut AC. Satu lapisan. Muat dalam satu PR. "Selesai bila" dapat diperiksa. Tidak menambah aturan bisnis. Dependensi merujuk ID yang ada. Nama tabel berprefix `master_` atau `trans_`. Tabel baru memuat enam kolom audit di migrasi yang sama |
| Architecture | Jawaban tech discovery disertai bukti. Klaim tanpa bukti ditandai. Keputusan merujuk DR. Nama tabel baru berprefix `master_` atau `trans_` dan entitasnya terdaftar di pengelompokan prefix. Tabel baru memuat enam kolom audit di migrasi yang sama |
| Decision Record | Konteks, opsi, keputusan, dan dampak ada. Nama dan role pengambil keputusan ditulis |
| Development status | Setiap folder story di setiap epic tercantum, dan tidak ada story yang sudah dihapus. Setiap file task tercantum satu baris, dengan judul dan tautan yang benar. Status story dan status task cocok dengan `status` di file task saat ini, menurut aturan `references/development-status.md`. Angka ringkasan (epic, story per status, task DONE) cocok dengan isi file. `last-update` diperbarui setiap kali story atau task berubah |

**Gaya tulisan** (cukup yang mengganggu pemahaman)

- Kata yang tidak dapat diukur di kebutuhan: "cepat", "mudah", "user-friendly", "aman", "optimal", "seamless". Ganti dengan angka dan kondisi, atau jadikan pertanyaan.
- Kata ragu di kebutuhan: "sebaiknya", "dapat dipertimbangkan", "mungkin". Ganti dengan "harus", atau jadikan pertanyaan.
- Satu konsep dengan beberapa nama ("peserta", "user", "kandidat"). Pakai satu istilah, sesuai glossary PRD.
- Kalimat pembuka atau penutup yang tidak berisi informasi ("Dokumen ini akan menjelaskan ...", "Dalam era digital ..."). Hapus.
- Rencana di luar cakupan ("Ke depannya bisa ditambah ..."). Hapus, atau pindahkan ke Out of scope.

## Kesiapan handoff ke engineering (per story)

Story siap bila semua butir ini terpenuhi:

1. README story ada dan setiap butir perilaku punya sumber.
2. `acceptance-criteria.md` ada dan setiap butir perilaku diuji minimal oleh satu AC.
3. Tidak ada pertanyaan **blocking** yang menyebut story ini, di PRD §10 maupun di README story.
4. Bila section desain menyebut perlu desain: `DESIGN.md` atau `layout.html` ada.
5. PRD tidak tertinggal dari brief (lihat baris PRD di atas).
6. `llms.txt` memuat tautan ke dokumen story ini.

Setelah task dibuat, periksa juga: setiap AC ditutup minimal oleh satu task, dan setiap AC yang dirujuk task benar-benar ada.

## Format laporan

Tulis dalam Bahasa Indonesia:

```markdown
## Review: <path atau story>

**Kesimpulan:** Siap | Perlu perbaikan kecil | Perlu perbaikan besar. <Satu kalimat alasan.>

### Temuan
- **[blocker]** L<n>: <masalah>. Usulan: <perbaikan konkret>.
- **[major]** L<n>: ...
- **[minor]** L<n>: "<kutipan persis>" → "<usulan>".

### Pertanyaan untuk pemilik dokumen (<role>)
- <Hal yang hanya bisa diputuskan pemilik dokumen.>
```

- `blocker`: dokumen tidak dapat diserahkan. Contoh: AC tidak dapat diuji, aturan dikarang, perilaku tanpa AC, pertanyaan blocking diabaikan.
- `major`: role berikutnya harus bertanya dulu sebelum dapat bekerja.
- `minor`: kata atau format.
- Urutkan dari yang paling berat. Kutip teks persis dengan nomor barisnya. Gabungkan temuan yang berulang menjadi satu ("'optimal' muncul 4× di L3, L9, L15, L22"). Tulis temuan saja, paling banyak 20.
- Saat memperbaiki atas permintaan user: perbaiki kata dan format langsung, tetapi jangan memutuskan substansi. Aturan, angka, atau pemilik yang hilang menjadi pertanyaan. Naikkan `version` bila dokumennya brief atau PRD.
