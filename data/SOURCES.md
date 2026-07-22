# Hán Nôm data provenance

The checked-in builder consumes four repository-local source snapshots:

- `Unihan_Readings.txt` for `kVietnamese` Hán-Việt readings.
- `NomStandardization.csv` for aligned Quốc ngữ and Chữ Nôm entries.
- `cake_gao_chunom.chars.dict.yaml` and `chu_nom.dict.yaml` for bundled Nôm
  character and curated phrase data.

`scripts/build_nom_dict.rs` preserves the source snapshots as inputs and
generates the binary dictionaries deterministically. This repository does not
assert redistribution terms for a new external corpus: document and verify its
licence before adding or replacing a source file.
