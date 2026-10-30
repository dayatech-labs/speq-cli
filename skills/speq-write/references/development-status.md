# Development Status

Path: `development-status.md` di root repository specs, sejajar `llms.txt`. Satu file untuk seluruh produk: setiap story di setiap epic, beserta status pengembangannya yang dihitung dari `status` task-nya. File ini ringkasan turunan; sumber kebenarannya tetap frontmatter `status` di setiap task. Semua role boleh menulisnya (`conventions.md` §Izin per role).

## Kapan diperbarui

Perbarui `development-status.md` di perubahan yang sama (commit yang sama) setiap kali:

- story ditambah, diganti nama, atau dihapus;
- task ditambah, diganti nama, atau dihapus;
- `status` sebuah task berubah, termasuk saat task selesai diimplementasi;
- file ini belum ada di repository: buat, lalu tambahkan satu baris untuk file ini di `llms.txt` section *Architecture & implementation*.

Perubahan lain (brief, PRD, AC, desain, architecture) tidak mengubah file ini.

Selalu susun ulang **seluruh** file dari isi repository, jangan menambal baris yang ada. Dengan begitu story lama dan story baru sama-sama tercantum, story yang dihapus hilang, dan angka ringkasan selalu cocok dengan tabel.

## Cara menyusun

1. Pastikan repository sudah di-`speq pull` (SKILL.md langkah 1) dan perubahan story atau task kamu sendiri sudah ditulis ke disk.
2. **Epic:** setiap folder di `epics/`, urut nama folder. Setiap epic mendapat satu baris di tabel epic dan satu section `## <epic>`. Epic tanpa folder `stories/` tetap ditulis, dengan baris `Belum ada story.` di section-nya.
3. **Story:** setiap folder `epics/<epic>/stories/st-NNN-<nama>/`, urut nomor. Satu folder adalah satu story, ada `README.md` atau belum.
   - Judul: heading `#` pertama di `README.md`, tanpa awalan `st-NNN — `. Bila `README.md` belum ada, pakai nama folder tanpa tautan.
4. **Task:** setiap file di `tasks/` story itu, urut jenis lalu nomor (`be-001`, `be-002`, `fe-001`). Satu task menjadi satu baris tabel.
   - Judul: heading `#` pertama di file task, tanpa awalan `<id task> — `. Bila tidak ada heading, pakai nama file tanpa `.md`.
   - Status: `status` dari frontmatter-nya. Task tanpa `status`, atau dengan nilai di luar `TODO`, `IN-PROGRESS`, `BLOCKED`, `DONE`, ditulis dan dihitung `TODO`. Sebutkan task itu ke user; jangan mengubah file task untuk menyesuaikannya.
5. **Status story** = aturan pertama yang cocok:

| Status story | Bila |
| --- | --- |
| `NO-TASK` | Story belum punya task |
| `DONE` | Semua task `DONE` |
| `BLOCKED` | Minimal satu task `BLOCKED` |
| `IN-PROGRESS` | Minimal satu task `IN-PROGRESS` atau `DONE` |
| `TODO` | Semua task `TODO` |

6. **`last-update`:** waktu file ini ditulis, dalam UTC dan format ISO 8601, diambil dari jam sistem: `date -u +%Y-%m-%dT%H:%M:%SZ` (PowerShell: `(Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")`). Jangan menebak waktu.
7. **Ringkasan:** jumlah epic (setiap baris tabel epic, termasuk epic tanpa story), jumlah story per status, dan jumlah task `DONE` dari seluruh task, dihitung dari isi file yang baru saja disusun.
8. **Tabel epic:** satu baris per epic, urutan sama dengan section-nya.
   - **Epic:** tautan ke section epic itu, `[<epic>](#<epic>)`. Nama folder epic sudah huruf kecil dengan `-`, jadi anchor-nya sama dengan nama folder.
   - **PRD:** tautan `[prd.md](epics/<epic>/prd.md)`, dokumen utama epic. Bila `prd.md` belum ada, tulis `—`, jangan tautan.
   - **Story DONE:** jumlah story `DONE` / jumlah story epic itu.
   - **Task DONE:** jumlah task `DONE` / jumlah task epic itu.

Status story hanya berasal dari task di repository. Jangan memakai status tiket Dayaboard, estimasi, atau tebakan. Arahnya satu: task → file ini. Jangan mengubah `status` task agar cocok dengan file ini.

## Format

```markdown
---
last-update: 2026-10-30T08:15:00Z
---

# Development Status

Status pengembangan setiap story, dihitung dari `status` task-nya. Disusun ulang setiap kali story atau task berubah.

**Ringkasan:** 2 epic · 4 story: 1 DONE, 1 IN-PROGRESS, 1 BLOCKED, 0 TODO, 1 NO-TASK · task DONE 4/7

| Epic | PRD | Story DONE | Task DONE |
| --- | --- | --- | --- |
| [landing-page](#landing-page) | [prd.md](epics/landing-page/prd.md) | 0/1 | 1/2 |
| [mcat](#mcat) | [prd.md](epics/mcat/prd.md) | 1/3 | 3/5 |

## landing-page

| Story | Status story | Task DONE | Task | Status task |
| --- | --- | --- | --- | --- |
| [st-001 — Hero section](epics/landing-page/stories/st-001-hero-section/README.md) | IN-PROGRESS | 1/2 | [fe-001 — Buat komponen hero](epics/landing-page/stories/st-001-hero-section/tasks/fe-001-komponen-hero.md) | DONE |
| | | | [fe-002 — Pasang animasi hero](epics/landing-page/stories/st-001-hero-section/tasks/fe-002-animasi-hero.md) | IN-PROGRESS |

## mcat

| Story | Status story | Task DONE | Task | Status task |
| --- | --- | --- | --- | --- |
| [st-001 — Login peserta](epics/mcat/stories/st-001-login-peserta/README.md) | DONE | 2/2 | [be-001 — Buat endpoint login](epics/mcat/stories/st-001-login-peserta/tasks/be-001-endpoint-login.md) | DONE |
| | | | [fe-001 — Buat halaman login](epics/mcat/stories/st-001-login-peserta/tasks/fe-001-halaman-login.md) | DONE |
| [st-002 — Pengerjaan section](epics/mcat/stories/st-002-pengerjaan-section/README.md) | BLOCKED | 1/3 | [be-001 — Simpan jawaban per section](epics/mcat/stories/st-002-pengerjaan-section/tasks/be-001-simpan-jawaban.md) | DONE |
| | | | [fe-001 — Tampilkan timer section](epics/mcat/stories/st-002-pengerjaan-section/tasks/fe-001-timer-section.md) | BLOCKED |
| | | | [qa-001 — Siapkan data uji section](epics/mcat/stories/st-002-pengerjaan-section/tasks/qa-001-data-uji-section.md) | TODO |
| [st-003 — Hasil tes](epics/mcat/stories/st-003-hasil-tes/README.md) | NO-TASK | 0/0 | — | — |
```

- Frontmatter hanya berisi `last-update` (`conventions.md` §Frontmatter).
- Tabel epic berada tepat setelah **Ringkasan**, tanpa heading sendiri, sebelum section epic pertama.
- Tautan relatif dari root repository: ke `prd.md` epic di kolom **PRD**, ke `README.md` story di kolom **Story**, dan ke file task di kolom **Task**. Kolom **Epic** menaut ke section di file ini.
- Satu baris per task. **Story**, **Status story**, dan **Task DONE** diisi hanya di baris task pertama story itu; baris task berikutnya mengosongkan ketiga sel itu. Story tanpa task tetap satu baris, dengan `—` di kolom **Task** dan **Status task**.
- Kolom **Task** berisi `<id task> — <judul>`.
- Tidak ada kolom, tabel, atau section lain. Catatan, rencana, atau blocker ditulis di dokumen story dan task-nya, bukan di sini.
