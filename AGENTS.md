# AGENTS.md

## Quality gate

Every change must pass all four, run from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p remote-rebirth-core --no-default-features
```

This last run compiles the core test suite without the optional `cli` feature; it must stay green
because the GUI links core with `default-features = false`.

Tests use the fake Remote and a fixed clock: no hardware, no network, no real `~/.config` or
`~/.local` state. New behaviour needs a test at the same seam as its neighbours.

## Agent skills

### Issue tracker

Local markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical five-role vocabulary. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/`. See `docs/agents/domain.md`.
