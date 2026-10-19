# CadSlate2D — shortcuts for the everyday commands in README.md#development.
# Every target wraps a script or a pnpm script; none adds logic of its own.
# `make` alone lists them.

SHELL := bash
.SHELLFLAGS := -e -o pipefail -c
.DEFAULT_GOAL := help

# rustup installs cargo outside PATH for non-login shells.
export PATH := $(HOME)/.cargo/bin:$(PATH)

WEB := pnpm --dir web
# Fuzzing: seconds per target, and which targets (all when empty).
FUZZ_SECONDS ?= 60
FUZZ_TARGETS ?=

.PHONY: help install dev wasm wasm-release build preview \
	check test test-rust test-web regression lint lint-rust lint-web \
	fmt fmt-check typecheck boundaries e2e e2e-only baselines \
	bench stress fuzz clean

help: ## List the targets
	@awk 'BEGIN { FS = ":.*## " } /^[a-z][a-z0-9-]*:.*## / { printf "  %-14s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# ── Setup and running ──

install: ## Install the web dependencies (first run)
	$(WEB) install

dev: ## Build the WASM core (debug) and start the Vite dev server
	$(WEB) dev

wasm: ## Rebuild the WASM core (debug) after changing Rust code
	$(WEB) wasm

wasm-release: ## Build the WASM core (release)
	$(WEB) wasm:release

build: ## Production bundle in web/dist/ (release WASM, typecheck, Vite)
	$(WEB) build

preview: ## Serve the production bundle
	$(WEB) preview

# ── Checks ──

check: ## The full check: run before every push
	scripts/check.sh

test: test-rust test-web ## Rust and web unit tests

test-rust: ## Rust tests
	cargo test --workspace

test-web: ## Web unit tests (Vitest)
	$(WEB) test

regression: ## The geometry regression corpus only
	cargo test -p cad-geometry --test regression

lint: lint-rust lint-web ## Clippy, ESLint, and Prettier

lint-rust: ## Clippy, warnings as errors
	cargo clippy --workspace --all-targets -- -D warnings

lint-web: ## ESLint and Prettier
	$(WEB) lint

typecheck: ## TypeScript typecheck
	$(WEB) typecheck

fmt: ## Format Rust and web sources
	cargo fmt --all
	$(WEB) format

fmt-check: ## Check formatting without changing anything
	cargo fmt --all --check
	$(WEB) exec prettier --check .

boundaries: ## The crate-layering check (ADR-0001)
	scripts/check-boundaries.sh

e2e: build ## End-to-end tests against a fresh production bundle (E2E_BROWSERS=…)
	$(WEB) e2e

e2e-only: ## End-to-end tests against the bundle already built
	$(WEB) e2e

baselines: build ## Re-record the visual baselines (Chromium); look at them before committing
	E2E_BROWSERS=chromium $(WEB) exec playwright test --update-snapshots

# ── Long runs, not part of the check ──

bench: ## Rust benchmarks
	cargo bench -p cad-benchmarks

stress: ## The geometry property tests with many more cases
	scripts/stress.sh

fuzz: ## Coverage-guided fuzzing (nightly, cargo-fuzz): FUZZ_SECONDS=60 FUZZ_TARGETS="…"
	scripts/fuzz.sh $(FUZZ_SECONDS) $(FUZZ_TARGETS)

# ── Housekeeping ──

clean: ## Remove build output: target/, web/dist/, and the generated WASM package
	cargo clean
	rm -rf web/dist web/src/cad/pkg
