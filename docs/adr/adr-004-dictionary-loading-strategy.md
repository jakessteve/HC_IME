# ADR-004: Dictionary Loading Strategy

- **Status:** accepted
- **Date:** 2026-07-26
- **Context:**
  `include_bytes!` embeds 468 KB of dictionary data (`han_nom_dict.bin` + `han_nom_phrase_dict.bin`) into the binary. On Linux/macOS these pages are COW-shared across processes via mmap/dyld. On Windows each process gets a private copy (468 KB × N processes). `get_global_dict()` at `han_nom.rs:326` already supports file-based loading via `HC_IME_NOM_DICT` env var. The refactoring plan adds cross-platform path resolution (ADR-002), making file-based loading viable on all platforms.
- **Decision:**
  Keep `include_bytes!` as build-time default (zero-config, works for `cargo test`). Add `platform::dictionary_paths()` search with `memmap2` for zero-copy runtime loading. The file path is checked first (via `HC_IME_NOM_DICT` env var AND `platform::dictionary_paths()`), falling back to `include_bytes!` when no file is found. On Windows, package managers should install the `.bin` files to `%ProgramData%/hcime/data/` so the file path is found.
- **Consequences:**
  - Positive: Windows avoids per-process duplication, dict updates don't require recompilation, zero-copy via `memmap2`.
  - Negative: Adds `memmap2` dependency.
  - Neutral: Embedded fallback preserves current behavior; all existing workflows (`cargo test`, development builds) continue unchanged.
- **Alternatives Considered:**
  - Remove `include_bytes!` entirely: Rejected — breaks `cargo test` and zero-config development by requiring pre-installed dictionary files.
  - File-only loading: Rejected — requires packaging coordination across three platforms; fragile when the file is missing.
  - Compile-time code generation into Rust arrays: Rejected — large generated files, slow compilation, no runtime-update benefit.
