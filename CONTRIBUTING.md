# Contributing to CadSlate2D

Thank you. CadSlate2D is a free, browser-native, local-first 2D CAD program in the spirit of classic command-driven drafting. This page says how a change gets in.

## Before you write code

1. **Read the sources of truth, in order:** [`docs/DESIGN.md`](docs/DESIGN.md) (the baseline; **LOCKED** items are not reopened without an ADR), [`docs/adr/`](docs/adr/README.md) (the decisions touching your area), and the GitHub issues (the plan, in order, with each issue's scope and acceptance criteria).
2. **Start from an issue.** Work is planned in issues grouped into milestones. If none covers what you want to do, open one first and say what and why; a large change needs an ADR before code.
3. **Read [`AGENTS.md`](AGENTS.md).** It holds the architecture rules that are easy to break, the engineering principles, the testing rules, and a table of where each kind of thing goes. It applies to people and to coding agents alike.

## Building and checking

| Step | Command |
| --- | --- |
| Prerequisites | Rust (rustup), `wasm-bindgen-cli` at the pinned version, Node 22+, pnpm 10+: see [docs/development.md](docs/development.md) |
| First run | `make install`, then `make dev` |
| The full check | `make check` |
| End-to-end tests | `make e2e` |
| Everything else | `make` lists the targets |

Every pull request must pass `make check`. CI runs it too.

## What a change needs

- **Tests, in the same commit** (DESIGN §41): geometry changes need unit and property tests and a regression case per fixed bug; commands need headless tests through `Session`; DXF changes need fixtures; UI changes need Vitest or Playwright coverage. [`AGENTS.md`](AGENTS.md#where-things-go) says which for each kind of change.
- **Documentation, in the same commit:** `docs/commands.md` (a command's row), `README.md`, `AGENTS.md`, the DXF or modify matrices, and `docs/roadmap.md` where they are affected.
- **A commit message that says what and why,** referencing the issue, and `Closes #N` when every acceptance criterion is met.
- **No new logic in the Makefile** — put it in `scripts/`.

## The workflow

- **The maintainer commits to `main` directly.** That is the project's own way of working and is not a model for contributors.
- **Everyone else contributes by pull request** against `main`, from a fork. Keep a pull request to one issue where practical. The CI check and the CLA check must pass; the maintainer reviews and merges.
- **Commit messages:** a short title in the imperative or as a statement of what changed, a body that explains why, and the issue reference.

## The Contributor License Agreement

The repository is licensed [AGPL-3.0-or-later](LICENSE) ([ADR-0035](docs/adr/0035-licence.md)). To keep the project's licensing coherent, contributors sign the [Contributor License Agreement](CLA.md) once: it grants the maintainer a licence to your contribution, including under other licences, and it leaves your own copyright with you.

The CLA check on your first pull request will ask you to sign by commenting on it. Pull requests from unsigned authors are not merged.

## Reporting a security issue

Please do not open a public issue for a vulnerability: use GitHub's private vulnerability reporting for this repository. `SECURITY.md` (#298) will say more.

## Conduct

Be straightforward and kind. The project is small; disagreement goes to the issue, with reasons. See [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) once it exists (#298).
