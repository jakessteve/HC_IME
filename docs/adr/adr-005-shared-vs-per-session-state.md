# ADR-005: Shared vs Per-Session State

- **Status:** accepted
- **Date:** 2026-07-26
- **Context:**
  Three types of data are duplicated per session: (1) `macros: HashMap<String, String>` — loaded from config, identical across sessions; (2) `phrase_history_path: PathBuf` — pure function of env vars, identical; (3) `phrase_history: PhraseHistory` — loaded from disk, then mutated per-session with user selections. PhraseHistory caps at 2,048 entries (~200 KB). If 20 Firefox tabs create 20 sessions, that's 4 MB of duplicated phrase history + 20 duplicate macro HashMaps. On Windows TSF (one instance per thread, 4 GB machines common in Vietnam), memory pressure from duplication is material.
- **Decision:**
  (1) Macros: `OnceLock<Arc<HashMap<String, String>>>` global, `Arc::clone` per session. (2) Phrase history path: `OnceLock<PathBuf>` global. (3) PhraseHistory: lazy-load on first Han Nom keystroke per session (not at session creation), using a global `OnceLock` for the initial disk load result, then per-session copy for mutations. Pure Vietnamese sessions never load it.
- **Consequences:**
  - Positive: 0 KB phrase history in pure Vietnamese mode, shared macros eliminate HashMap duplication across sessions.
  - Negative: Slightly more complex initialization — lazy-load guard on first Han Nom keystroke adds one branch per keystroke in Han Nom mode.
  - Neutral: Phrase history mutations remain per-session correct; one session's selections don't affect another's.
- **Alternatives Considered:**
  - Fully shared mutable state with `Arc<RwLock>`: Rejected — mutations from one session would affect all others, breaking per-document selection context.
  - Per-session everything, no sharing: Rejected — current state, wastes memory proportional to session count.
