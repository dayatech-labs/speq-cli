# Decision Record

Path: `epics/<epic>/decisions/dr-NNN-<judul>.md`. Decision Record (DR) dibuat untuk keputusan bisnis, arsitektur, atau cakupan yang signifikan, bukan untuk setiap revisi teks. Tidak memakai frontmatter.

Role `analyst`, `qa`, `ui_ux`, dan `engineer` dapat menulis DR. Keputusan milik PO ditulis oleh `analyst`.

Tulis hanya keputusan yang benar-benar sudah diambil, dengan nama pengambilnya. Opsi yang belum dibahas tim tidak boleh ditulis seolah sudah dipertimbangkan. Bila keputusan belum diambil, tulis pertanyaannya di PRD §10, bukan DR.

## Kerangka

```markdown
# dr-NNN — <Keputusan dalam satu kalimat>

| | |
| --- | --- |
| **Diputuskan oleh** | <nama dan role> |
| **Tanggal** | <YYYY-MM-DD> |
| **Terkait** | <st-NNN, BR-NN, atau pertanyaan yang dijawab> |
| **Menggantikan** | <dr-NNN, atau —> |

## Konteks

<Masalah yang memaksa keputusan ini, dengan fakta dan batasannya.>

## Opsi

| Opsi | Kelebihan | Kekurangan |
| --- | --- | --- |
| A. | | |
| B. | | |

## Keputusan

<Opsi yang dipilih dan alasannya.>

## Dampak

- <Dokumen, story, atau sistem yang berubah karena keputusan ini.>
```
