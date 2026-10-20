# CLAUDE.md

@AGENTS.md

## Claude Code specifics

- The shared rules live in `AGENTS.md` (imported above). Add project guidance there, not here, so every agent sees it.
- Before pushing, run `make check` (the same as `scripts/check.sh`) and report failures with their output rather than working around them.
- Working as the maintainer's agent, commit to `main` directly. A contribution on someone else's behalf goes by pull request, as `CONTRIBUTING.md` says.
- Close issues from commits (`Closes #N`) or with `gh issue close N --comment ...`; tick acceptance criteria only when a test or recorded evidence backs them.
- Rust is installed with rustup; if `cargo` is not on `PATH` in a fresh shell, source `~/.cargo/env`.
