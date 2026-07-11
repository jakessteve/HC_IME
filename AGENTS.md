# HC_IME Agent Rules

## Delegation Policy

**All code implementation must be delegated to subagents.**

The main agent (PM/orchestrator) must:
1. Classify the task and create a task contract
2. Delegate implementation to an appropriate subagent via the `task` tool
3. Review the subagent's output
4. Never write code directly, even for "simple" bug fixes

### Why This Rule Exists

Previous session violated this by implementing tone placement fixes directly instead of delegating. This bypasses the SOL pipeline and review gates.

### Subagent Selection

- `general` - General-purpose implementation
- `dev-fe` - Frontend-specific work
- `debugger` - Bug reproduction and root cause analysis
- `explore` - Codebase exploration and research

### Example Correct Flow

```
User: Fix bug X
PM: Classify → Create contract → Delegate to subagent
Subagent: Implement fix → Run tests → Return results
PM: Review → Validate → Report to user
```

## Testing

- Run `cargo test` in `hc_core/` for unit tests
- Run `scripts/e2e-smoke.sh` for end-to-end validation
- All tests must pass before completion

## Build

- Rust core: `hc_core/`
- Fcitx5 addon: `linux_fcitx5/`
- Build: `cmake -B build -G Ninja && cmake --build build`
