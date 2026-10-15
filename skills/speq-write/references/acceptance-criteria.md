# Acceptance criteria (analyst dan qa)

Path: `epics/<epic>/stories/st-NNN-<nama>/acceptance-criteria.md`. AC menjelaskan hasil yang dapat diuji QA tanpa membaca kode. Tidak memakai frontmatter.

## Aturan isi

- Setiap butir di section Perilaku README story diuji minimal oleh satu AC. Judul AC menyebut yang diujinya: `(Perilaku 2)`, `(BR-01)`, atau `(PRD §7)`.
- Format **Given / When / Then**. *Then* adalah hasil yang dapat diamati: teks yang tampil, status HTTP, data yang tersimpan, URL tujuan, atau waktu yang terukur. Kata seperti "berjalan dengan baik", "sesuai", atau "cepat" tidak dapat diuji.
- Tulis juga kondisi gagal dan pengecualian yang penting: input tidak valid, waktu habis, koneksi putus, akses ditolak, data kosong.
- Bila pengujian butuh pencocokan teks, tetapkan aturannya (kata utuh atau bagian kata, peka huruf besar atau tidak). Contoh: mencari "CORE" juga akan cocok dengan "SCORE".
- AC tidak menyebut cara implementasi (nama tabel, komponen, library).
- Nilai yang belum diputuskan ditulis `menunggu: <ringkasan pertanyaan> (PRD §10)`. AC itu tidak boleh diberi angka tebakan.
- Section `Verification` diisi QA: matriks browser/perangkat, data uji, dan hasil uji per AC.

## Kerangka

```markdown
# Acceptance Criteria — st-NNN <Nama story>

### AC-01 — <hasil yang diuji> (Perilaku 1)

- **Given** <kondisi awal dan data>
- **When** <aksi pengguna atau kejadian>
- **Then** <hasil yang dapat diamati>

### AC-02 — <kondisi gagal> (Perilaku 1)

- **Given** <kondisi>
- **When** <aksi>
- **Then** <pesan atau perilaku yang terlihat, dan data yang tidak berubah>

## Verification

<Diisi qa: matriks browser/perangkat, data uji, dan hasil uji per AC.>
```
