# Acuity Runtime

This repo contains a minimal FRAME runtime called `Acuity Runtime` (`acuity-runtime`), intended to be run with `polkadot-omni-node`.

It targets `edition = "2024"`, uses the crates.io `polkadot-sdk` umbrella at **2609.0.0**, and builds with the **nightly** toolchain pinned in `rust-toolchain.toml` (see [ARCHITECTURE.md](./ARCHITECTURE.md) for the toolchain and stable-escape-hatch details).

## Build the runtime

```bash
just build
```

This produces the wasm blob at:

`target/release/wbuild/acuity-runtime/acuity_runtime.wasm`

## Generate a chain spec

```bash
just chain-spec
```

## Run with Omni Node

Use `just` to rebuild the runtime, regenerate the chain spec, and start a local dev node:

```bash
just dev-node
```

The recipe writes a fresh chain spec to `target/dev-chain-spec.json` and starts `polkadot-omni-node` with `--dev`, `--dev-block-time 1000`, and `--state-pruning archive-canonical`.

If you want to see the available commands, use:

```bash
just --list
```

The underlying manual steps are:

```bash
just build

polkadot-omni-node chain-spec-builder \
  --chain-spec-path target/dev-chain-spec.json \
  create \
  -t development \
  --relay-chain rococo-local \
  --runtime target/release/wbuild/acuity-runtime/acuity_runtime.wasm \
  named-preset development

polkadot-omni-node \
  --chain target/dev-chain-spec.json \
  --dev \
  --dev-block-time 1000 \
  --state-pruning archive-canonical
```

The node runs in dev/manual-seal style mode and produces blocks locally (observed at 1s apart, which is why the recipe passes `--dev-block-time 1000`). The chain-spec command prints the output path when generation completes.

Note that `--dev-block-time` only affects the dev sealing driver: the runtime's own Aura slot duration is 6s (`MILLI_SECS_PER_BLOCK`).

## Test

```bash
just test-fast          # unit tests via cargo-nextest (default features)
just test-all-features  # same, with runtime-benchmarks enabled
just release-checks     # fmt --check, then clippy -D warnings, then test-fast
```

`cargo-nextest` is required for `test-fast` / `test-all-features` (`just install-nextest`); `just test-libtest` is the plain `cargo test` fallback.

## Benchmark with frame-omni-bencher

### Current status in this repository

`frame-omni-bencher` is installed (verified with v0.26.0).

This repository includes runtime benchmarking wiring: the `runtime-benchmarks` feature, the `frame_benchmarking::Benchmark` runtime API implementation, and entries in `define_benchmarks!` for `frame_system`, `pallet_balances`, `pallet_content`, `pallet_account_content`, `pallet_account_profile`, `pallet_content_reactions`, and `pallet_utility`.

The upstream `frame-benchmarking` no-std bug is still present in the crate shipped with `polkadot-sdk 2609.0.0` (`frame-benchmarking 50.0.0`): its `current_time()` calls `std::time::SystemTime` unconditionally, which breaks Wasm benchmark builds. `build.rs` patches that function in the cargo registry source automatically before invoking `substrate-wasm-builder`, so no manual machine-local patching is required.

### Expected benchmarking workflow

1. Build a benchmark-enabled Wasm runtime.
2. List available benchmarks.
3. Run the benchmark recipe to benchmark all pallets and write runtime-ready weight code.

Example commands:

```bash
# 1) Build benchmark-enabled runtime
just build-benchmarks

# 2) List available pallet/extrinsic benchmarks
frame-omni-bencher v1 benchmark pallet \
  --runtime target/release/wbuild/acuity-runtime/acuity_runtime.wasm \
  --list=all

# 3) Benchmark all runtime pallets and write weight files into src/weights
just benchmark

# 4) Verify the runtime still builds with the generated weights
just build
```

### Notes

- `just benchmark` builds with `runtime-benchmarks`, copies that wasm to `target/release/acuity-runtime-bench.wasm`, benchmarks against the copy, then rebuilds with a clean (feature-less) runtime. The copy exists because the final rebuild rewrites the `wbuild` path — benchmarking against `wbuild` afterwards fails with *"Did not find the benchmarking runtime api"*. Use `target/release/acuity-runtime-bench.wasm` for any follow-up single-pallet runs.
- `--steps` controls how many points are sampled across benchmark ranges; `--repeat` controls how many in-Wasm repetitions are run per sampled point. The recipe uses 50 and 20.
- To benchmark a subset, `frame-omni-bencher` takes `--pallets` and `--extrinsic` (use `"*"` for all extrinsics of the selected pallet).
- The recipe passes `templates/runtime-weight-template.hbs`, which is runtime-specific and emits code that compiles here without post-processing. It special-cases the component parameter names of `publish_item` and `publish_revision`.
- Generated files are written directly to `src/weights/` — currently `frame_system.rs`, `pallet_balances.rs`, `pallet_content.rs`, `pallet_account_content.rs`, `pallet_account_profile.rs`, `pallet_content_reactions.rs`, and `pallet_utility.rs`. Every one of those modules must stay listed in `src/weights/mod.rs`.
- Re-run the benchmarks after any `polkadot-sdk` bump or pallet change: measurements are toolchain- and machine-specific.
- If upstream fixes `frame-benchmarking`'s no_std `current_time()`, the temporary auto-patch in `build.rs` becomes a no-op and can be deleted.
