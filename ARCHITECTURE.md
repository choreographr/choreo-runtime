# Acuity Runtime Architecture

## Overview

Acuity Runtime is a **FRAME runtime** (`acuity-runtime`) written in Rust. It compiles to a Wasm blob for on-chain execution and a native `rlib` for tooling, and it is driven by `polkadot-omni-node`. The runtime uses FRAME from the Polkadot SDK, sourced as the published crates.io `polkadot-sdk` umbrella (**2609.0.0**) rather than a git tag.

It is wired as a **parachain**: `cumulus-pallet-parachain-system` owns the relay-chain context, the wasm exports the `validate_block` entry point, and the relay-chain inherents are part of every block. It is still a **dev-mode chain** in its economics — fees are zero on the weight axis and `Sudo` is composed in — and it has **no XCM stack yet** (see [Parachain wiring](#parachain-wiring)).

## Tech Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Toolchain | nightly, pinned in `rust-toolchain.toml` (stable via `scripts/build-stable.sh`) |
| Framework | Polkadot SDK `polkadot-sdk` v2609.0.0 (FRAME) |
| Encoding | Parity Scale Codec v3.7.5, scale-info v2.11.6 |
| Serialization | serde_json v1.0.154 (genesis presets) |
| Build | Cargo + `substrate-wasm-builder` |
| Task Runner | `just` (shell: Nushell) |
| Test Runner | `cargo-nextest` (libtest fallback available) |
| Node Runner | `polkadot-omni-node` |
| Benchmarking | `frame-omni-bencher` |
| Output | `cdylib` (Wasm blob) + `rlib` (native lib) |

## Directory Structure

```
acuity-runtime/
├── src/
│   ├── lib.rs              # Runtime definition, pallet configs, APIs, genesis, tests
│   └── weights/             # Benchmark-derived weight files per pallet
│       ├── mod.rs
│       ├── frame_system.rs
│       ├── pallet_balances.rs
│       ├── pallet_content.rs
│       ├── pallet_account_content.rs
│       ├── pallet_account_profile.rs
│       ├── pallet_content_reactions.rs
│       └── pallet_utility.rs
├── templates/
│   └── runtime-weight-template.hbs  # Handlebars template for weight file generation
├── scripts/
│   └── build-stable.sh      # One-shot stable-toolchain escape hatch
├── .cargo/config.toml        # profile-rustflags opt-in + nextest aliases
├── rust-toolchain.toml       # nightly default toolchain
├── build.rs                  # Build script: patches frame-benchmarking + invokes wasm-builder
├── Cargo.toml                # Crate manifest (dev/release/dist profiles)
├── justfile                  # Task recipes (build, chain-spec, dev-node, benchmark, tests)
├── ARCHITECTURE.md
└── README.md
```

## Pallet Architecture

The runtime wires together 14 pallets in `src/lib.rs` via the `#[frame_construct_runtime]` macro:

| Index | Runtime name | Pallet crate | Source | Responsibility |
|---|---|---|---|---|
| 0 | `System` | `frame_system` | polkadot-sdk | Block execution, events, account nonce; parachain prelude |
| 1 | `ParachainSystem` | `cumulus_pallet_parachain_system` | polkadot-sdk (cumulus) | Relay-chain context: validation data, relay parent number, messaging state, code upgrades |
| 2 | `Timestamp` | `pallet_timestamp` | polkadot-sdk | On-chain timestamp; drives Aura slot duration |
| 3 | `ParachainInfo` | `staging_parachain_info` | polkadot-sdk | Stores the `ParaId` |
| 4 | `Aura` | `pallet_aura` | polkadot-sdk | Block authorship consensus (sr25519 authorities) |
| 5 | `AuraExt` | `cumulus_pallet_aura_ext` | polkadot-sdk (cumulus) | Snapshots Aura authorities for the collator |
| 6 | `Balances` | `pallet_balances` | polkadot-sdk | Token balances, transfers, existential deposit |
| 7 | `Sudo` | `pallet_sudo` | polkadot-sdk | Superuser key (dev mode) |
| 8 | `TransactionPayment` | `pallet_transaction_payment` | polkadot-sdk | Transaction fee computation |
| 9 | `Content` | `pallet_content` | choreographr/pallet-content | Publish, revise, retract content items (IPFS hashes) |
| 10 | `AccountContent` | `pallet_account_content` | choreographr/pallet-content | Maps accounts to owned content items |
| 11 | `AccountProfile` | `pallet_account_profile` | choreographr/pallet-content | Sets an account's profile to a content item |
| 12 | `ContentReactions` | `pallet_content_reactions` | choreographr/pallet-content | Emoji reactions on content items (per revision) |
| 13 | `Utility` | `pallet_utility` | polkadot-sdk | Batch/union dispatch of calls |

### Parachain wiring

- `frame_system` uses `frame_system::config_preludes::ParaChainDefaultConfig` with `type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>`, so runtime code upgrades go through the parachain-system upgrade path rather than a local `set_code`.
- `cumulus_pallet_parachain_system::Config` is wired with `SelfParaId = ParachainInfo`, `CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases`, `ConsensusHook = consensus_hook::RequireParentIncluded` (the parent must be included before the next block is authored), `RelayParentOffset = ConstU32<0>`, and `SchedulingSignatureVerifier = ()` (V3 scheduling disabled, as upstream advises).
- `register_validate_block!` in `src/lib.rs` emits the `validate_block` export that relay-chain validators call; it expands to nothing under `std`. The built wasm contains that export.
- Every block must carry the `set_validation_data` inherent; `ParachainSystem::on_finalize` panics without it. The test suite supplies it through `cumulus-test-relay-sproof-builder`.

**No messaging yet.** `DmpQueue = ()`, `XcmpMessageHandler = ()`, `OutboundXcmpMessageSource = ()`, and both `ReservedDmpWeight` / `ReservedXcmpWeight` are zero. The consequences are deliberate and worth stating plainly:

- Inbound downward messages from the relay chain are **dropped**, not processed.
- The chain cannot send or receive XCMP messages; there is no HRMP channel support.
- Adding either means pulling in the XCM stack: `pallet-message-queue` plus `pallet-xcm` + `staging-xcm-builder` + `staging-xcm-executor` for DMP, and `cumulus-pallet-xcmp-queue` for XCMP. The message-queue `MessageProcessor` must be an `XcmExecutor`, so this is not a drop-in swap.

`ParachainSystem` has no generated weights (`type WeightInfo = ()`, the SDK's hand-written default); its benchmarks are not in `define_benchmarks!` yet.

### Weight wiring

Which `WeightInfo` each pallet uses:

| Pallet | Weight source |
|---|---|
| `frame_system`, `pallet_balances`, `pallet_content`, `pallet_account_content`, `pallet_account_profile`, `pallet_content_reactions`, `pallet_utility` | generated files in `src/weights/` |
| `pallet_timestamp`, `pallet_transaction_payment` | SDK `weights::SubstrateWeight<Runtime>` |
| `cumulus_pallet_parachain_system` | `()` — the SDK's hand-written default; not benchmarked here |
| `pallet_sudo` | `()` via its config prelude — unweighted, dev-only |
| `pallet_aura`, `pallet_aura_ext`, `staging_parachain_info` | no `WeightInfo` in their configs |

### Cross-Pallet Dependencies

```
Content ──────────────────┐
  │ ItemState storage       │
  │                         ▼
  │              AccountContent (verify ownership)
  │              AccountProfile (verify ownership)
  │              ContentReactions (verify item exists)
  │
Balances ←── System (AccountStore)
Timestamp ──→ Aura (OnTimestampSet)
Balances ←── TransactionPayment (FungibleAdapter for fees)
```

- `AccountContent`, `AccountProfile`, and `ContentReactions` all read `Content::ItemState` storage to verify item existence and ownership before mutating their own storage.
- `Balances` stores `AccountData` inside `System` via `type AccountStore = System`.
- `Timestamp` drives `Aura` via `type OnTimestampSet = Aura`.

## Runtime Configuration

### Constants

| Constant / binding | Value | Used By |
|---|---|---|
| `MILLI_SECS_PER_BLOCK` | 6000 (6s) | Aura slot duration; `Timestamp::MinimumPeriod` = half of it (3s) |
| `Aura::MaxAuthorities` | `ConstU32<32>` | Aura authority set bound |
| `Aura::AllowMultipleBlocksPerSlot` | `ConstBool<false>` | Aura |
| `ItemIdNamespace` | 1000 | Content item ID derivation |
| `MaxParents` | 32 | Content item parent links |
| `MaxLinks` | 128 | Content item links |
| `MaxMentions` | 256 | Content item mentions |
| `MaxItemsPerAccount` | 1024 | Account content item limit |
| `MaxEmojis` | 16 | Reaction emoji limit per user per (item, revision) |
| `ExistentialDeposit` | 1 | Balances minimum |
| `ParaId` (genesis `parachainInfo`) | 1000 | `ParachainInfo`; consumed by `ParachainSystem::SelfParaId` |
| `ReservedDmpWeight` / `ReservedXcmpWeight` | `Weight::zero()` | Nothing is reserved: no DMP/XCMP queues are wired |

### Fee Model (dev)

- **Weight-to-fee**: `NoFee` (zero) — computation weight costs nothing
- **Length-to-fee**: `ConstantMultiplier<Balance, ConstU128<1>>` — 1 unit per byte

So the only price signal is transaction length. Block weight is still bounded by `CheckWeight` for extrinsics that declare weight correctly — which is why every composed pallet needs a real `WeightInfo` rather than `()`; a zero declared weight cannot be gated up front.

### Genesis Preset

The single preset (`development`) configures:

- Alice and Bob with `1_000_000_000_000_000` balance each
- Alice as sole Aura authority
- Alice as Sudo key (dev only — remove `pallet_sudo` before any production use)
- `ParaId(1000)` in the `parachainInfo` genesis entry (the parachain-system genesis is left at its defaults)

`preset_names()` returns only this one preset; there is no `local`/`live` split yet.

## Transaction Pipeline

Extrinsics pass through `TxExtension` (signed transaction extension pipeline):

```
AuthorizeCall → CheckNonZeroSender → CheckSpecVersion → CheckTxVersion
→ CheckGenesis → CheckEra → CheckNonce → CheckWeight → ChargeTransactionPayment
→ WeightReclaim
```

## Runtime APIs

Implemented in `impl_runtime_apis!`:

- **Core** — version, execute_block, initialize_block
- **Metadata** — metadata, metadata_at_version, metadata_versions
- **BlockBuilder** — apply_extrinsic, finalize_block, inherent_extrinsics, check_inherents
- **TaggedTransactionQueue** — validate_transaction
- **OffchainWorkerApi** — offchain_worker
- **SessionKeys** — generate_session_keys, decode_session_keys (both return defaults; there is no session pallet)
- **AuraApi** — slot_duration, authorities
- **GetParachainInfo** — parachain_id (read from `ParachainInfo`)
- **AccountNonceApi** — account_nonce
- **TransactionPaymentApi** — query_info, query_fee_details, query_weight_to_fee, query_length_to_fee
- **GenesisBuilder** — build_state, get_preset, preset_names
- **Benchmark** (feature-gated) — benchmark_metadata, dispatch_benchmark

## Data Flows

### Content Publication

1. `Content::publish_item(origin, nonce, parents, flags, links, mentions, ipfs_hash)`
2. Item ID derived via `Blake2-256(account, nonce, ItemIdNamespace)`
3. `Content::ItemState` map populated with `Item { owner, revision_id, flags }`
4. `Content::publish_revision` bumps `revision_id`
5. `Content::retract_item` sets the `RETRACTED` flag (blocks further revisions)
6. `Content::set_not_revisionable` / `set_not_retractable` clear the corresponding permission flags

Flag bits: `REVISIONABLE = 1 << 0`, `RETRACTABLE = 1 << 1`, `RETRACTED = 1 << 2`.

### Account Content & Profile

1. `AccountContent::add_item(origin, item_id)` — verifies ownership via `Content::ItemState`, adds to the account's list
2. `AccountProfile::set_profile(origin, item_id)` — verifies ownership, sets it as the profile pointer
3. Only the item owner can add or profile their items (`WrongAccount` error otherwise)
4. `AccountContent::remove_item` removes an item from the list
5. Read helpers: `get_item_count`, `get_item_exists`, `get_all_items` (plus per-account variants)

### Content Reactions

1. `ContentReactions::set_reactions(origin, item_id, revision_id, reactions)` — verifies the item exists, then replaces the caller's reaction set for that revision
2. Reactions are per-revision: the same user can react differently to different revisions
3. Maximum `MaxEmojis` (16) emojis per account per (item, revision); duplicates rejected with `DuplicateEmoji`
4. An empty vector clears the caller's reactions

## Build System

### Toolchain and profiles

- `rust-toolchain.toml` pins `channel = "nightly"` so every plain `cargo` invocation gets the fast per-profile flags without the caller doing anything.
- `.cargo/config.toml` opts in to `[unstable] profile-rustflags`, which lets `Cargo.toml` scope rustflags per profile. `[profile.dev]` applies `-Zshare-generics=yes`, `--jobs-frontend=0`, and `-C target-cpu=native` (the last one never reaches a shipped artifact or the wasm build).
- `[profile.release]` is `opt-level = 3` with no LTO, to keep local links fast; `[profile.dist]` adds `lto = "thin"`, `codegen-units = 1`, `incremental = false`, `strip = "symbols"` for shipped artifacts.
- Building on **stable** is an explicit opt-out: `scripts/build-stable.sh` temporarily strips the nightly-only config/manifest keys, runs `cargo +stable <args>`, and restores them. Wired as `just build-stable` / `just check-stable` / `just test-stable`.

### Build Process

1. `cargo build --release` compiles the runtime
2. `build.rs` runs first: patches `frame-benchmarking`'s no-std bug, then invokes `substrate-wasm-builder`
3. Output: `target/release/wbuild/acuity-runtime/acuity_runtime.wasm` (plus `compact` and `compact.compressed` variants)

### Build Patch

`build.rs` rewrites `frame-benchmarking`'s `current_time()` to return `0u128.to_le_bytes()` under `not(feature = "std")`, which is what allows benchmark Wasm builds to succeed. The bug is still present in `frame-benchmarking 50.0.0`, the version pulled in by `polkadot-sdk 2609.0.0`.

The patch edits the crate source in the cargo registry (`$CARGO_HOME/registry/src/.../frame-benchmarking-*/src/utils.rs`) and in git checkouts, matching the old function body verbatim. Consequences worth knowing: the mutation is non-hermetic and persists in the shared registry cache, and it silently does nothing if upstream reflows that code. It prints a `cargo:warning=` only when it actually patches.

### Task Recipes (`justfile`)

| Recipe | Purpose |
|---|---|
| `build` | Compile runtime (`cargo build --release`) |
| `build-benchmarks` | Compile with the `runtime-benchmarks` feature |
| `chain-spec` | Generate dev chain spec using the built Wasm (depends on `build`) |
| `dev-node` | Regenerate chain spec and start local dev node (1s blocks, archive state pruning) |
| `benchmark` / `bench` | Full benchmark cycle: build with feature, copy wasm, run bencher, regenerate weights, rebuild clean |
| `run` | Alias of `dev-node` |
| `test` / `test-fast` | Unit tests via `cargo nextest run` (default features) |
| `test-all-features` | Unit tests with every optional feature on |
| `test-libtest` | Unit tests via plain `cargo test` |
| `install-nextest` | `cargo install cargo-nextest` |
| `release-checks` | `cargo fmt --check`, then `clippy --all-targets --all-features -D warnings`, then `test-fast` |
| `build-stable` / `check-stable` / `test-stable` | One-command stable-toolchain escape hatch |
| `about` / `menu` / `default` | Recipe listing and Nushell task selector |

### Benchmarking Pipeline

1. `cargo build --release --features runtime-benchmarks` produces a wasm exposing the Benchmark API
2. That wasm is copied to `target/release/acuity-runtime-bench.wasm`, because the recipe's final clean rebuild overwrites the `wbuild` path
3. `frame-omni-bencher v1 benchmark pallet --all --steps 50 --repeat 20 --template templates/runtime-weight-template.hbs --output src/weights` benchmarks against the copy
4. The template emits one Rust file per pallet implementing that pallet's `weights::WeightInfo` trait, using the `WeightInfo<T>` struct shape the runtime expects
5. The template special-cases the component parameter names of `publish_item` (three components) and `publish_revision` (two)
6. `cargo build --release` recompiles with the generated weights as a compile check

Weight measurements are machine- and toolchain-specific; regenerate after any SDK bump, pallet change, or hardware move.

## Dependency Policy

- `polkadot-sdk` is pinned to a single umbrella version used by both `[dependencies]` and `[build-dependencies]`.
- The content pallets (`pallet-content`, `pallet-account-content`, `pallet-account-profile`, `pallet-content-reactions`) come from `https://github.com/choreographr/pallet-content` on `branch = "master"`, locked by `Cargo.lock` to a specific commit.
- `cumulus-pallet-parachain-system` is declared **twice**: once as an umbrella feature (for the `polkadot_sdk::cumulus_pallet_parachain_system` re-export) and once as a direct dependency, because the `register_validate_block!` proc macro resolves its own crate by name from `Cargo.toml` via `proc-macro-crate`. Both must stay on the umbrella's version.
- `cumulus-test-relay-sproof-builder` is a dev-dependency only, used to fabricate validation data in tests.
- **The runtime and that repo must stay on the same `polkadot-sdk` major line.** The umbrella's `frame_support` / `frame_system` / `sp_io` re-exports are feature-gated, so if the two resolve different `polkadot-sdk` versions, cargo keeps two copies and the external pallets fail with `unresolved imports ... no frame_support in the root`. Following `branch = "master"` means an upstream push can trigger this without warning; pinning to a `rev` and building with `--locked` makes it deliberate.

## `no_std` Architecture

- `#![cfg_attr(not(feature = "std"), no_std)]` — compiles to Wasm without `std`
- Uses `extern crate alloc` and `alloc::vec::Vec`
- Wasm binary included via `include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"))` behind `#[cfg(feature = "std")]`

## Testing

The suite lives in `src/lib.rs` (`#[cfg(test)] mod tests`) and runs under `cargo-nextest` via `just test-fast` — 33 tests: 31 written in-crate plus the two macro-generated ones (`test_genesis_config_builds` and `__construct_runtime_integrity_test::runtime_integrity_tests`).

Coverage by area:

1. **Genesis presets** — development config contains expected balances, ParaId, sudo key, authority count; preset helpers expose/lookup correctly
2. **Runtime version** — spec/impl names and versions; API version list matches `RUNTIME_API_VERSIONS`
3. **Configuration constants** — all `parameter_types!` match expected values
4. **Metadata & config** — metadata encodes, ParaId, Aura authorities, slot duration
5. **Transaction fees** — zero weight fee, linear length fee
6. **Content flow** — publish, revise, retract, and retraction blocking
7. **Account content/profile** — ownership verification, add/remove items, set profile
8. **Content reactions** — per-revision tracking, emoji limits, duplicate rejection, clearing
9. **Content bounds** — BoundedVec limits for parents, links, mentions
10. **Balances** — transfers, insufficient balance, existential deposit
11. **Sudo** — genesis key is set correctly
12. **Content error paths** — invalid flags, duplicate publish, `set_not_revisionable`, `set_not_retractable`, wrong owner, nonexistent item
13. **Account content error paths** — duplicate add, remove not-added, retracted item rejection
14. **Account profile overwrites** — profile pointer can be replaced
15. **Content reactions error paths** — nonexistent item/revision, invalid emoji, duplicate no-op, remove absent, retracted item
16. **Utility batch** — batch dispatch of multiple calls
17. **Balances accounting** — free balances update after transfers

18. **Signed extrinsics** — a real signed extrinsic built and signed the way a client does, applied through `TxExtension`/`Executive`: nonce bump, length-fee charge, block-weight accounting (extension weight + call weight + base extrinsic weight), emitted events, and block finalization; plus a future nonce rejected as `InvalidTransaction::Future` and a replay rejected as `Stale`

The two block-level tests call `set_validation_data()` first, which fabricates a relay-chain storage proof with `cumulus-test-relay-sproof-builder` (a dev-dependency) and dispatches the inherent through `create_inherent` + `dispatch_bypass_filter`. Without it `finalize_block` panics, because a parachain block cannot finalize without validation data.

Not covered yet: `check_inherents` against a full candidate block, and actual relay-chain validation of the `validate_block` entry point (that needs a relay chain).

`just release-checks` is the release gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo test-fast`.
