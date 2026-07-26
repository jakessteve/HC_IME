# ADR-002: Platform Abstraction Layer

- **Status:** accepted
- **Date:** 2026-07-26
- **Context:**
  The codebase has zero cross-platform abstraction. Paths use Linux-only `XDG_STATE_HOME`, `HOME`, `/usr/share/...`. File permissions use Unix `PermissionsExt::from_mode(0o700)`. Only `cfg(unix)` guards exist (4 locations in `han_nom.rs`). Future macOS (IMK) and Windows (TSF) frontends need standardized directory resolution for dictionaries, configuration, and state files.
- **Decision:**
  Introduce `hc_core/src/platform.rs` using the `dirs = "5"` crate for cross-platform standard directories. Add functions: `data_dir()`, `config_dir()`, `state_dir()`, `dictionary_paths()`. Replace all manual `std::env::var("XDG_STATE_HOME")` calls with `platform::state_dir()`.
- **Consequences:**
  - Positive: Single source of truth for paths, new platforms add one match arm, no scattered `cfg` blocks.
  - Negative: Adds `dirs` dependency (~10 KB).
  - Neutral: Existing Linux paths are preserved as the default platform arm; behavior is identical on Linux.
- **Alternatives Considered:**
  - Manual cfg blocks per platform: Rejected — error-prone, duplicates logic across 10+ call sites, easy to miss one.
  - Environment variables only: Rejected — doesn't work well on Windows where standard paths are API-resolved (e.g., `SHGetKnownFolderPath`), not env-var based.
