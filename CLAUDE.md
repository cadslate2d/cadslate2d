# CLAUDE.md

@AGENTS.md

## Claude Code specifics

- The shared rules live in `AGENTS.md` (imported above). Add project guidance there, not here, so every agent sees it.
- Before pushing, run `scripts/check.sh` and report failures with their output rather than working around them.
- Close issues from commits (`Closes #N`) or with `gh issue close N --comment ...`; tick acceptance criteria only when a test or recorded evidence backs them.
- Rust is installed with rustup; if `cargo` is not on `PATH` in a fresh shell, source `~/.cargo/env`.
