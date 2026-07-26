# ADR-003: Additive FFI Migration Protocol

- **Status:** accepted
- **Date:** 2026-07-26
- **Context:**
  The FFI surface has 22 `extern "C"` functions. 10 are Han-Nom-specific (v1, v2, v3). The plan proposes replacing 6 versioned result types with a unified type. `HC_KeyRequest.input_mode: i32` passes `InputMode` enum values (0–5). Any renaming of enum variants or field layout changes will cause silent ABI corruption across the dynamic library boundary.
- **Decision:**
  Use additive-then-subtractive protocol: (1) Add new types/functions alongside old ones, (2) Migrate C++ consumer to new interface, (3) Validate with `e2e-smoke.sh` ABI checks, (4) Mark old types `#[deprecated]`, (5) Remove only after full validation. For `InputMode`, add `HC_KeyRequestV2` with separate `composition_method` and `translation_target` fields; keep `HC_KeyRequest` for backward compat.
- **Consequences:**
  - Positive: Bisectable, zero-downtime migration, rollback-safe — old consumer still works while new one is being validated.
  - Negative: Temporary type duplication during migration window.
  - Neutral: Old types linger for 1–2 release cycles; cleanup is a deliberate follow-up task.
- **Alternatives Considered:**
  - Swap types in-place: Rejected — silent ABI mismatch can cause heap corruption or segmentation faults in the C++ consumer.
  - Bump major FFI version atomically: Rejected — unnecessary breakage for consumers that don't need the new fields, harder to bisect regressions across the boundary.
