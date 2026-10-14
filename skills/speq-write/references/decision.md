# Decision Record

Path: `epics/<epic>/decisions/dr-NNN-<judul>.md`. Decision Record (DR) dibuat untuk keputusan bisnis, arsitektur, atau cakupan yang signifikan. DR tidak diperlukan untuk setiap revisi teks.

AI hanya menulis DR berstatus `proposed`. Status `accepted` diisi oleh pemilik keputusan. Opsi yang belum benar-benar dibahas tim tidak boleh ditulis seolah sudah dipertimbangkan.

## Kerangka

```markdown
---
id: <epic>-dr-NNN
type: decision
epic: <epic>
owner: <role pemilik keputusan>
status: proposed
decided_at:
drafted_by: AI
updated: YYYY-MM-DD
---

# dr-NNN — <Keputusan dalam satu kalimat>

| | |
| --- | --- |
| **Pemilik keputusan** | <nama atau role> |
| **Terkait** | <OQ-NN, FR-NN, st-NNN> |
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
