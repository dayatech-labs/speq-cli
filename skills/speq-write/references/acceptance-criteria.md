# Acceptance criteria (Analyst + QA)

Path: `epics/<epic>/stories/st-NNN-<nama>/acceptance-criteria.md`. AC menjelaskan hasil yang dapat diuji QA tanpa membaca kode.

## Aturan isi

- Setiap FR dan NFR story diuji minimal satu AC. Setiap AC menyebut FR/NFR yang diujinya di judulnya.
- Format **Given / When / Then**. *Then* adalah hasil yang dapat diamati: teks yang tampil, status HTTP, data yang tersimpan, URL tujuan, atau waktu yang terukur. Kata seperti "berjalan dengan baik", "sesuai", atau "cepat" tidak dapat diuji.
- Tulis juga kondisi gagal dan pengecualian yang penting: input tidak valid, waktu habis, koneksi putus, hak akses ditolak, data kosong.
- AC tidak menyebut cara implementasi (nama tabel, komponen, library).
- Nilai yang belum diputuskan ditulis `menunggu OQ-NN`. AC tersebut tidak boleh diberi angka tebakan.
- Section `Verification` milik QA. Saat drafting, isi hanya placeholder kutipan seperti di kerangka.

## Kerangka

```markdown
---
id: <epic>-st-NNN-ac
type: acceptance-criteria
epic: <epic>
story: <epic>-st-NNN
owner: Analyst, QA
status: draft
drafted_by: AI
updated: YYYY-MM-DD
---

# Acceptance Criteria — st-NNN <Nama story>

### AC-01 — <hasil yang diuji> (FR-NN)

- **Given** <kondisi awal dan data>
- **When** <aksi pengguna atau kejadian>
- **Then** <hasil yang dapat diamati>

### AC-02 — <kondisi gagal> (FR-NN)

- **Given** <kondisi>
- **When** <aksi>
- **Then** <pesan atau perilaku yang terlihat, dan data yang tidak berubah>

## Verification

> Diisi QA: matriks browser/perangkat, data uji, dan hasil uji per AC.
```
