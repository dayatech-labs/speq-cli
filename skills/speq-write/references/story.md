# Story (Analyst)

Path: `epics/<epic>/stories/st-NNN-<nama>/README.md`. Buat satu folder per story yang tercantum di PRD §5. Nomor FR melanjutkan FR tertinggi di epic.

## Aturan isi

- **FR** menjelaskan satu perilaku yang dapat diamati, dengan sumbernya: `*(Must|Should|Could · brief: <section>, DoD <n>)*`. FR yang berisi dua perilaku dipecah menjadi dua.
- **Aturan** khusus story ditulis di section 4. Aturan lintas story cukup dirujuk ke PRD (`BR-NN`).
- Perilaku yang belum diputuskan ditulis sebagai FR dengan rujukan `menunggu OQ-NN`, bukan diisi tebakan.
- `needs_design: false` bila story tidak mengubah UI. Section 7 tetap diisi alasannya.

## Kerangka

```markdown
---
id: <epic>-st-NNN
type: story
epic: <epic>
owner: Analyst
status: draft
requirements: [FR-NN, NFR-NN]
needs_design: true
drafted_by: AI
updated: YYYY-MM-DD
---

# st-NNN — <Nama story>

**Sebagai** <role>, **saya ingin** <kemampuan>, **agar** <manfaat>.

| | |
| --- | --- |
| **Pengguna** | <role> |
| **Goal** | G<n> |
| **Prasyarat** | <story lain atau dependensi, atau —> |
| **Dokumen** | [acceptance criteria](acceptance-criteria.md) · [tasks](tasks/) · desain: <tautan atau "belum ada"> · [PRD](../../prd.md) |

## 1. Konteks

<Posisi story di alur epic dan BR yang berlaku.>

## 2. User flow

1. <Langkah yang dilakukan pengguna dan respons sistem.>

## 3. Functional requirements

- **FR-NN** <perilaku>. *(Must · brief: <section>, DoD <n>)*

## 4. Aturan

1. <Aturan khusus story ini.>

## 5. Di luar ruang lingkup

- <Hal yang sengaja tidak dikerjakan, dengan rujukan story lain bila ada.>

## 6. Open questions dan dependensi

- OQ-NN — <ringkasan> (**blocking**).
- Dependensi: <data, layanan, atau story lain>.

## 7. Desain

UIUX: <layar dan state yang perlu didesain, atau "Tidak ada perubahan UI" beserta alasannya>.
```
