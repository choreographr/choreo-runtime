set shell := ["nu", "-c"]

runtime_wasm := "target/release/wbuild/acuity-runtime/acuity_runtime.wasm"
# The benchmark-enabled wasm is kept at a separate path: the recipe's trailing
# `cargo build --release` (features off) rewrites `runtime_wasm`, so a bencher
# run against that path afterwards would fail with "Did not find the benchmarking
# runtime api". Copying it out lets you re-run single-pallet benchmarks right
# after `just benchmark` without rebuilding.
runtime_wasm_bench := "target/release/acuity-runtime-bench.wasm"
chain_spec := "target/dev-chain-spec.json"

default:
    @just _task-selector

_task-selector:
    #!/usr/bin/env nu
    let selected_task = (
        just --summary -u
        | split row ' '
        | to text
        | fzf --header 'Available recipes' --header-first --layout reverse --preview 'just --show {}'
        | if ($in | is-empty) { 'about' } else { $in }
    )
    just $selected_task

menu:
    @just _task-selector

@about:
    just --list

build:
    cargo build --release

build-benchmarks:
    cargo build --release --features runtime-benchmarks

chain-spec: build
    if ((which polkadot-omni-node | length) == 0) { error make { msg: 'polkadot-omni-node is not installed or not on PATH' } }
    polkadot-omni-node chain-spec-builder \
      --chain-spec-path {{chain_spec}} \
      create \
      -t development \
      --relay-chain rococo-local \
      --runtime {{runtime_wasm}} \
      named-preset development
    print $'Wrote chain spec to {{chain_spec}}'

dev-node: chain-spec
    if ((which polkadot-omni-node | length) == 0) { error make { msg: 'polkadot-omni-node is not installed or not on PATH' } }
    polkadot-omni-node \
      --chain {{chain_spec}} \
      --dev \
      --dev-block-time 1000 \
      --state-pruning archive-canonical

benchmark:
    if ((which frame-omni-bencher | length) == 0) { error make { msg: 'frame-omni-bencher is not installed or not on PATH' } }
    cargo build --release --features runtime-benchmarks
    cp {{runtime_wasm}} {{runtime_wasm_bench}}
    frame-omni-bencher v1 benchmark pallet \
      --runtime {{runtime_wasm_bench}} \
      --all \
      --steps 50 \
      --repeat 20 \
      --template templates/runtime-weight-template.hbs \
      --output src/weights \
      --quiet
    cargo build --release

run: dev-node

bench: benchmark

# ── stable builds (escape hatch from the nightly default) ─────────────────────
# The default toolchain is NIGHTLY (see rust-toolchain.toml), so every adhoc
# `cargo` command — including `cargo build` / `cargo check` / `cargo test` —
# automatically applies the fast per-profile `-Z` flags via [unstable]
# profile-rustflags (see .cargo/config.toml). Those same nightly-only bits
# HARD-BLOCK stable Cargo, so a stable build is an explicit opt-out: these
# recipes run through scripts/build-stable.sh, which temporarily strips the
# nightly-only config/manifest keys for one command and restores them after.

# Stable build (the `profile` — release by default — plus any extra args)
build-stable:
    ./scripts/build-stable.sh build

# Stable type-check (all targets, no linking)
check-stable:
    ./scripts/build-stable.sh check --all-targets

# Stable tests (libtest)
test-stable:
    ./scripts/build-stable.sh test

# ── testing (cargo-nextest, the primary runner) ──────────────────────────────
# The default toolchain is NIGHTLY (see rust-toolchain.toml), so all of these
# auto-apply the fast per-profile `-Z` flags. The `test-*` recipes use
# cargo-nextest (parallel, every test in its own process); the cargo aliases are
# defined in .cargo/config.toml. A libtest fallback is provided for the
# nextest-less path, and `test-stable` covers the stable escape hatch.

# Install the test runner (cargo-nextest)
install-nextest:
    cargo install cargo-nextest

# Fail fast with a hint when cargo-nextest is missing (beats cargo's "no such
# command: test-fast")
_require-nextest:
    if ((which cargo-nextest | length) == 0) { error make { msg: 'cargo-nextest is not installed — run `just install-nextest`' } }

# Full unit suite (alias of test-fast)
test: test-fast

# Unit tests via nextest (parallel, every test in its own process; default
# feature set)
test-fast: _require-nextest
    cargo test-fast

# Unit tests with every optional feature on (runtime-benchmarks etc.)
test-all-features: _require-nextest
    cargo test-all-features

# Unit tests via libtest (no nextest required)
test-libtest:
    cargo test

# Release gate: format, lint, and the full unit suite.
release-checks:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    just test-fast
