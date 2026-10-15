# Story (analyst)

Path: `epics/<epic>/stories/st-NNN-<nama>/README.md`. Buat satu folder per story yang tercantum di PRD §5. Story tidak memakai frontmatter.

## Aturan isi

- **Perilaku** adalah daftar bernomor (1, 2, 3, ...) tentang apa yang dilakukan sistem untuk pengguna. Satu butir berisi satu perilaku yang dapat diamati, dengan sumbernya: `*(brief: <section>, DoD <n>)*`. Butir yang berisi dua perilaku dipecah menjadi dua. Jangan memberi ID seperti FR-01.
- **Aturan** khusus story ditulis di section 4. Aturan lintas story cukup dirujuk ke PRD (`BR-NN`).
- Perilaku yang belum diputuskan ditulis apa adanya dengan `menunggu: <ringkasan pertanyaan> (PRD §10)`, bukan diisi tebakan.
- Data yang menentukan perilaku (daftar nama, band skor, tabel aturan) disalin sebagai tabel di section 3 atau 4. Lihat `conventions.md` §Sumber di luar repository.
- Section 7 menyebut apakah story butuh desain, dan layar atau state mana yang perlu didesain.

## Kerangka

```markdown
# st-NNN — <Nama story>

**Sebagai** <role pengguna>, **saya ingin** <kemampuan>, **agar** <manfaat>.

| | |
| --- | --- |
| **Pengguna** | <role pengguna> |
| **Goal** | G<n> |
| **Prasyarat** | <story lain atau dependensi, atau —> |
| **Dokumen** | [acceptance criteria](acceptance-criteria.md) · [tasks](tasks/) · desain: <tautan atau "belum ada"> · [PRD](../../prd.md) |

## 1. Konteks

<Posisi story di alur epic dan BR yang berlaku.>

## 2. User flow

1. <Langkah yang dilakukan pengguna dan respons sistem.>

## 3. Perilaku

1. <Perilaku yang dapat diamati>. *(brief: <section>, DoD <n>)*

## 4. Aturan

1. <Aturan khusus story ini.>

## 5. Di luar ruang lingkup

- <Hal yang sengaja tidak dikerjakan, dengan rujukan story lain bila ada.>

## 6. Pertanyaan terbuka dan dependensi

- [ ] <Pertanyaan yang hanya menyangkut story ini> (ditanyakan ke: <role>)
- Pertanyaan lintas story: lihat PRD §10.
- Dependensi: <data, layanan, atau story lain>.

## 7. Desain

<"Perlu desain: <layar dan state>." atau "Tidak ada perubahan UI: <alasan>.">
```
