#![cfg_attr(not(feature = "std"), no_std)]
// `frame_construct_runtime!` recurses deeply with this many pallets.
#![recursion_limit = "1024"]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

use alloc::vec::Vec;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::weights::ConstantMultiplier;
use pallet_transaction_payment::{FeeDetails, RuntimeDispatchInfo};
use polkadot_sdk::{
    polkadot_sdk_frame::{
        self as frame,
        deps::sp_genesis_builder,
        runtime::{apis, prelude::*},
    },
    staging_parachain_info as parachain_info,
    staging_xcm::latest::prelude::AssetId,
    *,
};
use sp_session::OpaqueGeneratedSessionKeys;

pub mod genesis_config_presets;
pub mod governance;
pub mod parameters;
pub mod supply_guard;
pub mod weights;
pub mod xcm_config;

// Primitive aliases, re-exported so submodules can `use crate::…`. These must
// stay identical to what `frame_system::config_preludes::ParaChainDefaultConfig`
// would pick, and are restated explicitly below so the runtime does not depend
// on a prelude's defaults.
pub type AccountId = sp_runtime::AccountId32;
pub type Signature = sp_runtime::MultiSignature;
pub type Balance = u128;
pub type Nonce = u32;
pub type BlockNumber = u32;
pub type Hash = sp_core::H256;

#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("choreographr-runtime"),
    impl_name: alloc::borrow::Cow::Borrowed("choreographr-runtime"),
    authoring_version: 1,
    spec_version: 1,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};

// --- timing -------------------------------------------------------------------

pub const MILLI_SECS_PER_BLOCK: u64 = 6000;
pub const SLOT_DURATION: u64 = MILLI_SECS_PER_BLOCK;

pub const MINUTES: BlockNumber = 60_000 / (MILLI_SECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

// --- denominations ------------------------------------------------------------
//
// CHOR carries 18 decimals: that preserves the ACU ledger's raw integers, so the
// 1:1 balance snapshot needs no rescaling, and it matches the ERC-20 convention
// for any future bridged representation.

pub const UNITS: Balance = 1_000_000_000_000_000_000;
pub const CENTS: Balance = UNITS / 100;
pub const MILLI_UNIT: Balance = 1_000_000_000_000_000;
pub const MICRO_UNIT: Balance = 1_000_000_000_000;

/// Existential deposit: 0.01 CHOR.
pub const EXISTENTIAL_DEPOSIT: Balance = 10 * MILLI_UNIT;

/// Fee per unit of `ref_time` weight: 0.000000000001 CHOR.
pub const WEIGHT_FEE_MULTIPLIER: Balance = 1_000_000;

parameter_types! {
    /// Fee per byte of extrinsic length: 0.00001 CHOR.
    pub const TransactionByteFee: Balance = 10 * MICRO_UNIT;
}

/// Convert weight into a fee. A linear mapping keeps fees predictable; swap in a
/// polynomial if the chain ever needs to price congestion.
pub type WeightToFee = ConstantMultiplier<Balance, ConstU128<WEIGHT_FEE_MULTIPLIER>>;

// --- consensus / block limits -------------------------------------------------

/// Async-backing parameters. These must not exceed the target relay chain's
/// `async_backing_params` (`max_candidate_depth` = 3, `allowed_ancestry_len` = 2
/// on Polkadot).
pub(crate) const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
pub(crate) const BLOCK_PROCESSING_VELOCITY: u32 = 1;
pub(crate) const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;

pub type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
    Runtime,
    RELAY_CHAIN_SLOT_DURATION_MILLIS,
    BLOCK_PROCESSING_VELOCITY,
    UNINCLUDED_SEGMENT_CAPACITY,
>;

/// ~5% of block weight is assumed to be consumed by `on_initialize`.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);
/// `Normal` extrinsics may fill 75% of the block; the rest is operational space.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Two seconds of compute at a six-second block time.
const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
    frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
    cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
);

parameter_types! {
    pub const SS58PrefixValue: u16 = 76;
    pub RuntimeBlockLength: frame_system::limits::BlockLength =
        frame_system::limits::BlockLength::builder()
            .max_length(5 * 1024 * 1024)
            .modify_max_length_for_class(DispatchClass::Normal, |m| {
                *m = NORMAL_DISPATCH_RATIO * *m
            })
            .build();
    pub RuntimeBlockWeights: frame_system::limits::BlockWeights =
        frame_system::limits::BlockWeights::builder()
            .base_block(frame_support::weights::constants::BlockExecutionWeight::get())
            .for_class(DispatchClass::all(), |weights| {
                weights.base_extrinsic =
                    frame_support::weights::constants::ExtrinsicBaseWeight::get();
            })
            .for_class(DispatchClass::Normal, |weights| {
                weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
            })
            .for_class(DispatchClass::Operational, |weights| {
                weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
                weights.reserved =
                    Some(MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
            })
            .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
            .build_or_panic();
}

/// No runtime-wide migrations yet. Add `OnRuntimeUpgrade` items here as the
/// runtime evolves \u2014 notably the `ParachainInfo` rewrite needed when this chain
/// moves from its own relay chain onto Polkadot.
type SingleBlockMigrations = ();

pub type TxExtension = cumulus_pallet_weight_reclaim::StorageWeightReclaim<
    Runtime,
    (
        frame_system::AuthorizeCall<Runtime>,
        frame_system::CheckNonZeroSender<Runtime>,
        frame_system::CheckSpecVersion<Runtime>,
        frame_system::CheckTxVersion<Runtime>,
        frame_system::CheckGenesis<Runtime>,
        frame_system::CheckEra<Runtime>,
        frame_system::CheckNonce<Runtime>,
        frame_system::CheckWeight<Runtime>,
        pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    ),
>;

#[frame_construct_runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system::Pallet<Runtime>;

    #[runtime::pallet_index(1)]
    pub type ParachainSystem = cumulus_pallet_parachain_system::Pallet<Runtime>;

    #[runtime::pallet_index(2)]
    pub type Timestamp = pallet_timestamp::Pallet<Runtime>;

    #[runtime::pallet_index(3)]
    pub type ParachainInfo = parachain_info::Pallet<Runtime>;

    #[runtime::pallet_index(4)]
    pub type Aura = pallet_aura::Pallet<Runtime>;

    #[runtime::pallet_index(5)]
    pub type AuraExt = cumulus_pallet_aura_ext::Pallet<Runtime>;

    #[runtime::pallet_index(6)]
    pub type Balances = pallet_balances::Pallet<Runtime>;

    #[runtime::pallet_index(7)]
    pub type Sudo = pallet_sudo::Pallet<Runtime>;

    #[runtime::pallet_index(8)]
    pub type TransactionPayment = pallet_transaction_payment::Pallet<Runtime>;

    #[runtime::pallet_index(9)]
    pub type Content = pallet_content::Pallet<Runtime>;

    #[runtime::pallet_index(10)]
    pub type AccountContent = pallet_account_content::Pallet<Runtime>;

    #[runtime::pallet_index(11)]
    pub type AccountProfile = pallet_account_profile::Pallet<Runtime>;

    #[runtime::pallet_index(12)]
    pub type ContentReactions = pallet_content_reactions::Pallet<Runtime>;

    #[runtime::pallet_index(13)]
    pub type Utility = pallet_utility::Pallet<Runtime>;

    // --- collator / consensus ------------------------------------------------

    #[runtime::pallet_index(14)]
    pub type Authorship = pallet_authorship::Pallet<Runtime>;

    #[runtime::pallet_index(15)]
    pub type Session = pallet_session::Pallet<Runtime>;

    #[runtime::pallet_index(16)]
    pub type CollatorSelection = pallet_collator_selection::Pallet<Runtime>;

    #[runtime::pallet_index(17)]
    pub type WeightReclaim = cumulus_pallet_weight_reclaim::Pallet<Runtime>;

    // --- xcm -----------------------------------------------------------------

    #[runtime::pallet_index(18)]
    pub type XcmpQueue = cumulus_pallet_xcmp_queue::Pallet<Runtime>;

    #[runtime::pallet_index(19)]
    pub type PolkadotXcm = pallet_xcm::Pallet<Runtime>;

    #[runtime::pallet_index(20)]
    pub type CumulusXcm = cumulus_pallet_xcm::Pallet<Runtime>;

    #[runtime::pallet_index(21)]
    pub type MessageQueue = pallet_message_queue::Pallet<Runtime>;

    // --- governance ----------------------------------------------------------

    #[runtime::pallet_index(22)]
    pub type Preimage = pallet_preimage::Pallet<Runtime>;

    #[runtime::pallet_index(23)]
    pub type Scheduler = pallet_scheduler::Pallet<Runtime>;

    #[runtime::pallet_index(24)]
    pub type Referenda = pallet_referenda::Pallet<Runtime>;

    #[runtime::pallet_index(25)]
    pub type ConvictionVoting = pallet_conviction_voting::Pallet<Runtime>;

    #[runtime::pallet_index(26)]
    pub type Whitelist = pallet_whitelist::Pallet<Runtime>;

    #[runtime::pallet_index(27)]
    pub type Treasury = pallet_treasury::Pallet<Runtime>;

    #[runtime::pallet_index(28)]
    pub type Origins = governance::custom_origins::Pallet<Runtime>;

    #[runtime::pallet_index(29)]
    pub type Parameters = pallet_parameters::Pallet<Runtime>;
}

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;
    pub const ItemIdNamespace: u32 = 1000;
    pub const MaxParents: u32 = 32;
    pub const MaxLinks: u32 = 128;
    pub const MaxMentions: u32 = 256;
    pub const MaxItemsPerAccount: u32 = 1024;
    pub const MaxEmojis: u32 = 16;

    /// Weight reserved for processing downward messages from the relay chain.
    pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    /// Weight reserved for processing sibling (XCMP/HRMP) messages.
    pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;

    pub MessageQueueServiceWeight: Weight = Perbill::from_percent(35) * RuntimeBlockWeights::get().max_block;

    /// The asset used to pay message delivery fees to the relay chain.
    pub FeeAssetId: AssetId = AssetId(xcm_config::RelayLocation::get());
    pub const ToSiblingBaseDeliveryFee: u128 = 3 * CENTS;
    pub const ToParentBaseDeliveryFee: u128 = 3 * CENTS;
}

/// Price for delivering XCM to a sibling parachain (e.g. Hydration).
pub type PriceForSiblingParachainDelivery =
    polkadot_runtime_common::xcm_sender::ExponentialPrice<
        FeeAssetId,
        ToSiblingBaseDeliveryFee,
        TransactionByteFee,
        XcmpQueue,
    >;

/// Price for delivering XCM up to the relay chain.
pub type PriceForParentDelivery = polkadot_runtime_common::xcm_sender::ExponentialPrice<
    FeeAssetId,
    ToParentBaseDeliveryFee,
    TransactionByteFee,
    ParachainSystem,
>;

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
impl frame_system::Config for Runtime {
    type AccountId = AccountId;
    type Nonce = Nonce;
    type Hash = Hash;
    type Block = Block;
    type Version = Version;
    type AccountData = pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
    type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = RuntimeBlockLength;
    type SS58Prefix = SS58PrefixValue;
    type SingleBlockMigrations = SingleBlockMigrations;
    // Fixed-supply guard: rejects issuance-changing calls reached through any
    // origin that is not root, including via sudo, batches or scheduling.
    type BaseCallFilter = supply_guard::SupplyGuard;
}

impl cumulus_pallet_weight_reclaim::Config for Runtime {
    type WeightInfo = ();
}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnSystemEvent = ();
    type SelfParaId = ParachainInfo;
    type OutboundXcmpMessageSource = XcmpQueue;
    type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
    type ReservedDmpWeight = ReservedDmpWeight;
    type XcmpMessageHandler = XcmpQueue;
    type ReservedXcmpWeight = ReservedXcmpWeight;
    type CheckAssociatedRelayNumber =
        cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
    type ConsensusHook = ConsensusHook;
    type WeightInfo = ();
    type RelayParentOffset = ConstU32<0>;
    type SchedulingSignatureVerifier = ();
}

parameter_types! {
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_balances::Config for Runtime {
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = frame_support::traits::VariantCountOf<RuntimeFreezeReason>;
    type DoneSlashHandler = ();
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    // Async backing may author more than one block per relay slot.
    type AllowMultipleBlocksPerSlot = ConstBool<true>;
    type SlotDuration = ConstU64<SLOT_DURATION>;
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

impl pallet_timestamp::Config for Runtime {
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<{ MILLI_SECS_PER_BLOCK / 2 }>;
    type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
    type WeightToFee = WeightToFee;
    type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
    type FeeMultiplierUpdate = polkadot_runtime_common::SlowAdjustingFeeUpdate<Runtime>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_content::Config for Runtime {
    type WeightInfo = weights::pallet_content::WeightInfo<Runtime>;
    type ItemIdNamespace = ItemIdNamespace;
    type MaxParents = MaxParents;
    type MaxLinks = MaxLinks;
    type MaxMentions = MaxMentions;
}

impl pallet_account_content::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = weights::pallet_account_content::WeightInfo<Runtime>;
    type MaxItemsPerAccount = MaxItemsPerAccount;
}

impl pallet_account_profile::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = weights::pallet_account_profile::WeightInfo<Runtime>;
}

impl pallet_content_reactions::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = weights::pallet_content_reactions::WeightInfo<Runtime>;
    type MaxEmojis = MaxEmojis;
}

impl pallet_utility::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type PalletsOrigin = OriginCaller;
    type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

// --- collators ----------------------------------------------------------------

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type EventHandler = (CollatorSelection,);
}

polkadot_sdk::sp_runtime::impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
    }
}

parameter_types! {
    pub const Period: BlockNumber = 6 * HOURS;
    pub const Offset: BlockNumber = 0;
    pub const PotId: PalletId = PalletId(*b"chor/pot");
    pub const CollatorSessionLength: BlockNumber = 6 * HOURS;
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    // Collators serve as their own stash/controller pair.
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = CollatorSelection;
    type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type DisablingStrategy = ();
    type Currency = Balances;
    type KeyDeposit = ();
    type WeightInfo = weights::pallet_session::WeightInfo<Runtime>;
}

impl pallet_collator_selection::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    // Root (and therefore the root/general_admin tracks) manages the collator set.
    type UpdateOrigin = frame_system::EnsureRoot<AccountId>;
    type PotId = PotId;
    // Invulnerables-only: public candidacy is disabled (`MaxCandidates = 0`).
    // Enable candidacy by raising this and lowering `MinEligibleCollators`.
    type MaxCandidates = ConstU32<0>;
    type MinEligibleCollators = ConstU32<1>;
    type MaxInvulnerables = ConstU32<10>;
    // Must be a multiple of the session length or the set drifts out of sync.
    type KickThreshold = Period;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ValidatorRegistration = Session;
    type WeightInfo = pallet_collator_selection::weights::SubstrateWeight<Runtime>;
}

/// Session benchmarks need to mint a session key and prove ownership of it.
/// The implementation lives inside the `Benchmark` runtime API below, so it is
/// only compiled into the benchmark-enabled wasm.

// --- message queues / xcmp -----------------------------------------------------

impl pallet_message_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    #[cfg(feature = "runtime-benchmarks")]
    type MessageProcessor = pallet_message_queue::mock_helpers::NoopMessageProcessor<
        AggregateMessageOrigin,
    >;
    #[cfg(not(feature = "runtime-benchmarks"))]
    type MessageProcessor = polkadot_sdk::staging_xcm_builder::ProcessXcmMessage<
        AggregateMessageOrigin,
        polkadot_sdk::staging_xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
        RuntimeCall,
    >;
    type Size = u32;
    // The queue only ever sees `Sibling(ParaId)` origins from XCMP.
    type QueueChangeHandler =
        parachains_common::message_queue::NarrowOriginToSibling<XcmpQueue>;
    type QueuePausedQuery =
        parachains_common::message_queue::NarrowOriginToSibling<XcmpQueue>;
    type HeapSize = ConstU32<{ 103 * 1024 }>;
    type MaxStale = ConstU32<8>;
    type ServiceWeight = MessageQueueServiceWeight;
    type IdleMaxServiceWeight = ();
    type WeightInfo = pallet_message_queue::weights::SubstrateWeight<Runtime>;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ChannelInfo = ParachainSystem;
    type VersionWrapper = ();
    // Enqueue sibling messages into the shared message queue.
    type XcmpQueue = frame_support::traits::TransformOrigin<
        MessageQueue,
        AggregateMessageOrigin,
        ParaId,
        parachains_common::message_queue::ParaIdToSibling,
    >;
    type MaxInboundSuspended = ConstU32<1_000>;
    type MaxActiveOutboundChannels = ConstU32<128>;
    type MaxPageSize = ConstU32<{ 1 << 16 }>;
    type ControllerOrigin = frame_system::EnsureRoot<AccountId>;
    type ControllerOriginConverter = xcm_config::XcmOriginToTransactDispatchOrigin;
    type PriceForSiblingDelivery = PriceForSiblingParachainDelivery;
    type WeightInfo = cumulus_pallet_xcmp_queue::weights::SubstrateWeight<Runtime>;
}

// Parachains cannot use `frame_system_benchmarking`'s default hooks. `System::set_code`
// routes through `ParachainSetCode`, which schedules the upgrade via
// `ParachainSystem::schedule_code_upgrade` instead of writing `:code` directly. That path
// needs the `ValidationData` and `HostConfiguration` only the `set_validation_data`
// inherent writes, and it emits `ValidationFunctionStored` instead of `CodeUpdated`
// (`frame_system::update_code_in_storage`, the only emitter of `CodeUpdated`, is never
// reached). With the default no-op setup, `frame_system::set_code` and
// `frame_system::apply_authorized_upgrade` both abort with `ValidationDataNotAvailable` and
// the whole benchmark run produces no weights.
#[cfg(feature = "runtime-benchmarks")]
impl frame_system_benchmarking::Config for Runtime {
    fn setup_set_code_requirements(
        code: &Vec<u8>,
    ) -> Result<(), frame_benchmarking::BenchmarkError> {
        ParachainSystem::initialize_for_set_code_benchmark(code.len() as u32);
        Ok(())
    }

    fn verify_set_code() {
        System::assert_last_event(
            cumulus_pallet_parachain_system::Event::<Runtime>::ValidationFunctionStored.into(),
        );
    }
}

type Block = frame::runtime::types_common::BlockOf<Runtime, TxExtension>;
type Header = HeaderFor<Runtime>;
type RuntimeExecutive =
    Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllPalletsWithSystem>;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    use super::*;
    use frame_support::traits::{StorageInfo, WhitelistedStorageKeys};
    use polkadot_sdk::cumulus_pallet_session_benchmarking::Pallet as SessionBench;
    use polkadot_sdk::frame_system_benchmarking::Pallet as SystemBench;

    polkadot_sdk::frame_benchmarking::define_benchmarks!(
        [frame_system, SystemBench::<Runtime>]
        [pallet_account_content, AccountContent]
        [pallet_account_profile, AccountProfile]
        [pallet_balances, Balances]
        [pallet_collator_selection, CollatorSelection]
        [pallet_content, Content]
        [pallet_content_reactions, ContentReactions]
        [pallet_message_queue, MessageQueue]
        [pallet_session, SessionBench::<Runtime>]
        [pallet_timestamp, Timestamp]
        [pallet_utility, Utility]
        [cumulus_pallet_xcmp_queue, XcmpQueue]
    );

    pub fn benchmark_metadata(
        extra: bool,
    ) -> (Vec<frame_benchmarking::BenchmarkList>, Vec<StorageInfo>) {
        let mut list = Vec::<frame_benchmarking::BenchmarkList>::new();
        list_benchmarks!(list, extra);

        let storage_info = AllPalletsWithSystem::storage_info();
        (list, storage_info)
    }

    pub fn dispatch_benchmark(
        config: frame_benchmarking::BenchmarkConfig,
    ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
        let whitelist = AllPalletsWithSystem::whitelisted_storage_keys();
        let mut batches = Vec::<frame_benchmarking::BenchmarkBatch>::new();
        let params = (&&config, &whitelist);
        add_benchmarks!(params, batches);
        Ok(batches)
    }
}

impl_runtime_apis! {
    impl apis::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: <Block as frame::traits::Block>::LazyBlock) {
            RuntimeExecutive::execute_block(block)
        }

        fn initialize_block(header: &Header) -> ExtrinsicInclusionMode {
            RuntimeExecutive::initialize_block(header)
        }
    }

    impl apis::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }

        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }

        fn metadata_versions() -> Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl apis::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: ExtrinsicFor<Runtime>) -> ApplyExtrinsicResult {
            RuntimeExecutive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> HeaderFor<Runtime> {
            RuntimeExecutive::finalize_block()
        }

        fn inherent_extrinsics(data: InherentData) -> Vec<ExtrinsicFor<Runtime>> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: <Block as frame::traits::Block>::LazyBlock,
            data: InherentData,
        ) -> CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl apis::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: ExtrinsicFor<Runtime>,
            block_hash: <Runtime as frame_system::Config>::Hash,
        ) -> TransactionValidity {
            RuntimeExecutive::validate_transaction(source, tx, block_hash)
        }
    }

    impl apis::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &HeaderFor<Runtime>) {
            RuntimeExecutive::offchain_worker(header)
        }
    }

    impl apis::SessionKeys<Block> for Runtime {
        fn generate_session_keys(
            owner: Vec<u8>,
            seed: Option<Vec<u8>>,
        ) -> OpaqueGeneratedSessionKeys {
            SessionKeys::generate(&owner, seed).into()
        }

        fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, apis::KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
        }

        fn authorities() -> Vec<sp_consensus_aura::sr25519::AuthorityId> {
            pallet_aura::Authorities::<Runtime>::get().into_inner()
        }
    }

    impl cumulus_primitives_core::GetParachainInfo<Block> for Runtime {
        fn parachain_id() -> cumulus_primitives_core::ParaId {
            ParachainInfo::parachain_id()
        }
    }

    impl apis::AccountNonceApi<Block, interface::AccountId, interface::Nonce> for Runtime {
        fn account_nonce(account: interface::AccountId) -> interface::Nonce {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, interface::Balance>
        for Runtime {
        fn query_info(
            uxt: ExtrinsicFor<Runtime>,
            len: u32,
        ) -> RuntimeDispatchInfo<interface::Balance> {
            TransactionPayment::query_info(uxt, len)
        }

        fn query_fee_details(
            uxt: ExtrinsicFor<Runtime>,
            len: u32,
        ) -> FeeDetails<interface::Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }

        fn query_weight_to_fee(weight: Weight) -> interface::Balance {
            TransactionPayment::weight_to_fee(weight)
        }

        fn query_length_to_fee(length: u32) -> interface::Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl apis::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }

        fn get_preset(id: &Option<PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, self::genesis_config_presets::get_preset)
        }

        fn preset_names() -> Vec<PresetId> {
            self::genesis_config_presets::preset_names()
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(
            extra: bool,
        ) -> (
            Vec<frame_benchmarking::BenchmarkList>,
            Vec<frame_support::traits::StorageInfo>,
        ) {
            benches::benchmark_metadata(extra)
        }

        #[allow(non_local_definitions)]
        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig,
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
            // Session benchmarks mint a session key and must prove ownership of
            // it. Defined here so it only exists in the benchmark build.
            impl cumulus_pallet_session_benchmarking::Config for Runtime {
                fn generate_session_keys_and_proof(
                    owner: Self::AccountId,
                ) -> (Self::Keys, Vec<u8>) {
                    let generated = SessionKeys::generate(&owner.encode(), None);
                    (generated.keys, generated.proof.encode())
                }
            }

            benches::dispatch_benchmark(config)
        }
    }
}

pub mod interface {
    use super::Runtime;
    use polkadot_sdk::{polkadot_sdk_frame as frame, *};

    pub type Block = super::Block;
    pub use frame::runtime::types_common::OpaqueBlock;
    pub type AccountId = <Runtime as frame_system::Config>::AccountId;
    pub type Nonce = <Runtime as frame_system::Config>::Nonce;
    pub type Hash = <Runtime as frame_system::Config>::Hash;
    pub type Balance = <Runtime as pallet_balances::Config>::Balance;
}

// The relay-chain validation function used by parachain validators to execute a
// candidate block. `register_validate_block!` expands to nothing under `std`, so
// this only affects the wasm build.
cumulus_pallet_parachain_system::register_validate_block! {
    Runtime = Runtime,
    BlockExecutor = RuntimeExecutive,
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;
    use frame_support::{assert_noop, assert_ok};
    use pallet_content::{IpfsHash, ItemId, Nonce, RETRACTABLE, REVISIONABLE};
    use pallet_content_reactions::Emoji;
    use polkadot_sdk::{
        frame_support::BoundedVec,
        sp_application_crypto::Ss58Codec,
        sp_keyring::Sr25519Keyring,
        sp_runtime::{BuildStorage, DispatchError, TokenError, bounded_vec},
    };
    use serde_json::Value;

    type AccountId = interface::AccountId;

    fn alice() -> AccountId {
        Sr25519Keyring::Alice.to_account_id()
    }

    fn bob() -> AccountId {
        Sr25519Keyring::Bob.to_account_id()
    }

    fn charlie() -> AccountId {
        Sr25519Keyring::Charlie.to_account_id()
    }

    fn new_test_ext() -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .expect("frame system storage builds");

        RuntimeGenesisConfig {
            balances: BalancesConfig {
                balances: vec![
                    (alice(), 1_000_000 * UNITS),
                    (bob(), 1_000_000 * UNITS),
                    (charlie(), 1_000_000 * UNITS),
                ],
                dev_accounts: None,
            },
            aura: pallet_aura::GenesisConfig {
                authorities: vec![Sr25519Keyring::Alice.public().into()],
            },
            parachain_info: ParachainInfoConfig {
                parachain_id: cumulus_primitives_core::ParaId::new(1000),
                _config: Default::default(),
            },
            sudo: SudoConfig { key: Some(alice()) },
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .expect("runtime genesis assimilates");

        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    fn nonce_from_byte(b: u8) -> Nonce {
        let mut bytes = [0u8; 32];
        bytes[0] = b;
        codec::Decode::decode(&mut &bytes[..]).unwrap()
    }

    fn item_id_for(account: &AccountId, nonce: Nonce) -> ItemId {
        let mut item_id = ItemId::default();
        item_id.0.copy_from_slice(&sp_io::hashing::blake2_256(
            &[
                account.encode(),
                nonce.encode(),
                ItemIdNamespace::get().encode(),
            ]
            .concat(),
        ));
        item_id
    }

    fn publish_item(owner: AccountId, nonce: Nonce, flags: u8) -> ItemId {
        let item_id = item_id_for(&owner, nonce.clone());
        assert_ok!(Content::publish_item(
            RuntimeOrigin::signed(owner),
            nonce,
            Default::default(),
            flags,
            Default::default(),
            Default::default(),
            IpfsHash::default(),
        ));
        item_id
    }

    #[test]
    fn development_genesis_preset_contains_expected_values() {
        let patch = genesis_config_presets::development_config_genesis();

        assert_eq!(
            patch["balances"]["balances"],
            Value::Array(vec![
                Value::Array(vec![
                    Value::String(alice().to_ss58check()),
                    Value::Number(10_000_000_000_000_000_000u64.into()),
                ]),
                Value::Array(vec![
                    Value::String(bob().to_ss58check()),
                    Value::Number(10_000_000_000_000_000_000u64.into()),
                ]),
            ])
        );
        assert_eq!(
            patch["parachainInfo"]["parachainId"],
            Value::Number(1000u64.into())
        );
        assert_eq!(patch["sudo"]["key"], Value::String(alice().to_ss58check()));
        assert_eq!(
            patch["aura"]["authorities"].as_array().map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn genesis_preset_helpers_expose_development_preset() {
        let names = genesis_config_presets::preset_names();
        assert_eq!(
            names,
            vec![PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET)]
        );

        let preset = genesis_config_presets::get_preset(&PresetId::from(
            sp_genesis_builder::DEV_RUNTIME_PRESET,
        ))
        .expect("development preset exists");
        let parsed: Value = serde_json::from_slice(&preset).expect("preset bytes are valid json");
        assert_eq!(parsed["sudo"]["key"], Value::String(alice().to_ss58check()));

        assert_eq!(
            genesis_config_presets::get_preset(&PresetId::from("unknown-preset")),
            None
        );
    }

    #[test]
    fn runtime_version_is_consistent() {
        assert_eq!(VERSION.spec_name.as_ref(), "choreographr-runtime");
        assert_eq!(VERSION.impl_name.as_ref(), "choreographr-runtime");
        assert_eq!(VERSION.spec_version, 1);
        assert_eq!(VERSION.transaction_version, 1);
        assert_eq!(VERSION.apis.len(), RUNTIME_API_VERSIONS.len());
    }

    #[test]
    fn runtime_constants_match_expected_configuration() {
        assert_eq!(ItemIdNamespace::get(), 1000);
        assert_eq!(MaxParents::get(), 32);
        assert_eq!(MaxLinks::get(), 128);
        assert_eq!(MaxMentions::get(), 256);
        assert_eq!(MaxItemsPerAccount::get(), 1024);
        assert_eq!(MaxEmojis::get(), 16);
    }

    #[test]
    fn runtime_metadata_and_configuration_are_available() {
        new_test_ext().execute_with(|| {
            let metadata = Runtime::metadata();
            assert!(!metadata.encode().is_empty());
            assert!(!Runtime::metadata_versions().is_empty());

            assert_eq!(
                ParachainInfo::parachain_id(),
                cumulus_primitives_core::ParaId::new(1000)
            );
            assert_eq!(
                pallet_aura::Authorities::<Runtime>::get().into_inner(),
                vec![Sr25519Keyring::Alice.public().into()]
            );
            assert_eq!(Aura::slot_duration(), MILLI_SECS_PER_BLOCK);
        });
    }

    #[test]
    fn transaction_payment_uses_weight_and_length_fees() {
        new_test_ext().execute_with(|| {
            // Linear weight fee: ref_time * WEIGHT_FEE_MULTIPLIER.
            assert_eq!(
                TransactionPayment::weight_to_fee(Weight::from_parts(1_000, 0)),
                1_000 * WEIGHT_FEE_MULTIPLIER
            );
            // Linear length fee: bytes * TransactionByteFee.
            let byte_fee = TransactionByteFee::get();
            assert_eq!(TransactionPayment::length_to_fee(1), byte_fee);
            assert_eq!(TransactionPayment::length_to_fee(7), 7 * byte_fee);
            assert_eq!(TransactionPayment::length_to_fee(1024), 1024 * byte_fee);
        });
    }

    #[test]
    fn content_flow_works_in_runtime() {
        new_test_ext().execute_with(|| {
            let nonce = Nonce::default();
            let item_id = item_id_for(&alice(), nonce.clone());
            let mention: BoundedVec<AccountId, MaxMentions> = vec![bob()].try_into().unwrap();

            assert_ok!(Content::publish_item(
                RuntimeOrigin::signed(alice()),
                nonce,
                Default::default(),
                REVISIONABLE | RETRACTABLE,
                Default::default(),
                mention.clone(),
                IpfsHash([1; 32]),
            ));

            System::assert_has_event(
                pallet_content::Event::<Runtime>::PublishItem {
                    item_id: item_id.clone(),
                    owner: alice(),
                    parents: Default::default(),
                    flags: REVISIONABLE | RETRACTABLE,
                }
                .into(),
            );

            assert_ok!(Content::publish_revision(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
                Default::default(),
                Default::default(),
                IpfsHash([2; 32]),
            ));
            assert_eq!(
                pallet_content::ItemState::<Runtime>::get(&item_id)
                    .unwrap()
                    .revision_id,
                1
            );

            assert_ok!(Content::retract_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));
            assert!(
                pallet_content::ItemState::<Runtime>::get(&item_id)
                    .unwrap()
                    .flags
                    & pallet_content::RETRACTED
                    != 0
            );

            assert_noop!(
                Content::publish_revision(
                    RuntimeOrigin::signed(alice()),
                    item_id,
                    Default::default(),
                    Default::default(),
                    IpfsHash([3; 32]),
                ),
                pallet_content::Error::<Runtime>::ItemRetracted
            );
        });
    }

    #[test]
    fn account_content_and_profile_integrate_with_content_ownership() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_ok!(AccountContent::add_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));
            assert_eq!(AccountContent::get_item_count(alice()), 1);
            assert!(AccountContent::get_item_exists(alice(), item_id.clone()));

            assert_ok!(AccountProfile::set_profile(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));
            assert_eq!(
                pallet_account_profile::AccountProfile::<Runtime>::get(alice()),
                Some(item_id.clone())
            );

            assert_noop!(
                AccountContent::add_item(RuntimeOrigin::signed(bob()), item_id.clone()),
                pallet_account_content::Error::<Runtime>::WrongAccount
            );
            assert_noop!(
                AccountProfile::set_profile(RuntimeOrigin::signed(bob()), item_id.clone()),
                pallet_account_profile::Error::<Runtime>::WrongAccount
            );

            assert_ok!(AccountContent::remove_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));
            assert_eq!(AccountContent::get_item_count(alice()), 0);
            assert!(!AccountContent::get_item_exists(alice(), item_id));
        });
    }

    #[test]
    fn content_reactions_track_revisions_and_limits() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);
            assert_ok!(Content::publish_revision(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
                Default::default(),
                Default::default(),
                IpfsHash([9; 32]),
            ));

            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id.clone(),
                0,
                bounded_vec![Emoji(0x1F600)],
            ));
            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id.clone(),
                1,
                bounded_vec![Emoji(0x1F389)],
            ));

            System::assert_has_event(
                pallet_content_reactions::Event::<Runtime>::SetReactions {
                    item_id: item_id.clone(),
                    revision_id: 0,
                    item_owner: alice(),
                    reactor: bob(),
                    reactions: bounded_vec![Emoji(0x1F600)],
                }
                .into(),
            );
            System::assert_has_event(
                pallet_content_reactions::Event::<Runtime>::SetReactions {
                    item_id: item_id.clone(),
                    revision_id: 1,
                    item_owner: alice(),
                    reactor: bob(),
                    reactions: bounded_vec![Emoji(0x1F389)],
                }
                .into(),
            );

            let many_emojis: BoundedVec<Emoji, MaxEmojis> = (0x1F601..=0x1F610)
                .map(Emoji)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(charlie()),
                item_id.clone(),
                0,
                many_emojis,
            ));

            let duplicate_emojis: BoundedVec<Emoji, MaxEmojis> =
                vec![Emoji(0x1F600), Emoji(0x1F600)].try_into().unwrap();
            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(charlie()),
                    item_id.clone(),
                    0,
                    duplicate_emojis,
                ),
                pallet_content_reactions::Error::<Runtime>::DuplicateEmoji
            );

            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id.clone(),
                0,
                bounded_vec![],
            ));
            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id,
                1,
                bounded_vec![Emoji(0x1F389)],
            ));
        });
    }

    #[test]
    fn content_bounds_match_runtime_limits() {
        new_test_ext().execute_with(|| {
            let too_many_mentions: Vec<_> = (0..=MaxMentions::get()).map(|_| bob()).collect();
            assert!(BoundedVec::<AccountId, MaxMentions>::try_from(too_many_mentions).is_err());

            let too_many_parents = vec![ItemId([1; 32]); (MaxParents::get() + 1) as usize];
            assert!(BoundedVec::<ItemId, MaxParents>::try_from(too_many_parents).is_err());

            let too_many_links = vec![ItemId([2; 32]); (MaxLinks::get() + 1) as usize];
            assert!(BoundedVec::<ItemId, MaxLinks>::try_from(too_many_links).is_err());
        });
    }

    #[test]
    fn balances_transfer_and_existential_deposit() {
        new_test_ext().execute_with(|| {
            let initial = Balances::free_balance(alice());
            assert_eq!(initial, 1_000_000 * UNITS);

            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(alice()),
                bob().into(),
                500,
            ));
            assert_eq!(Balances::free_balance(alice()), initial - 500);
            assert_eq!(Balances::free_balance(bob()), 1_000_000 * UNITS + 500);

            assert_eq!(Balances::free_balance(charlie()), 1_000_000 * UNITS);
            assert_noop!(
                Balances::transfer_allow_death(
                    RuntimeOrigin::signed(charlie()),
                    alice().into(),
                    1_000_000 * UNITS + 1,
                ),
                DispatchError::Token(TokenError::FundsUnavailable)
            );

            let new_account: AccountId = sp_runtime::AccountId32::from([0x42u8; 32]);
            // Below the existential deposit: a new account cannot be created.
            assert_noop!(
                Balances::transfer_allow_death(
                    RuntimeOrigin::signed(alice()),
                    new_account.clone().into(),
                    EXISTENTIAL_DEPOSIT - 1,
                ),
                DispatchError::Token(TokenError::BelowMinimum)
            );
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(alice()),
                new_account.clone().into(),
                EXISTENTIAL_DEPOSIT,
            ));
            assert_eq!(Balances::free_balance(new_account), EXISTENTIAL_DEPOSIT);
        });
    }

    #[test]
    fn sudo_key_is_set_in_genesis() {
        new_test_ext().execute_with(|| {
            assert_eq!(pallet_sudo::Key::<Runtime>::get(), Some(alice()));
        });
    }

    #[test]
    fn content_publish_with_invalid_flags_fails() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                Content::publish_item(
                    RuntimeOrigin::signed(alice()),
                    Nonce::default(),
                    Default::default(),
                    0b111,
                    Default::default(),
                    Default::default(),
                    IpfsHash::default(),
                ),
                pallet_content::Error::<Runtime>::InvalidFlags
            );
        });
    }

    #[test]
    fn content_duplicate_publish_fails() {
        new_test_ext().execute_with(|| {
            let nonce = Nonce::default();
            assert_ok!(Content::publish_item(
                RuntimeOrigin::signed(alice()),
                nonce.clone(),
                Default::default(),
                REVISIONABLE | RETRACTABLE,
                Default::default(),
                Default::default(),
                IpfsHash::default(),
            ));

            assert_noop!(
                Content::publish_item(
                    RuntimeOrigin::signed(alice()),
                    nonce,
                    Default::default(),
                    REVISIONABLE | RETRACTABLE,
                    Default::default(),
                    Default::default(),
                    IpfsHash::default(),
                ),
                pallet_content::Error::<Runtime>::ItemAlreadyExists
            );
        });
    }

    #[test]
    fn content_set_not_revisionable_blocks_future_revisions() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_ok!(Content::publish_revision(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
                Default::default(),
                Default::default(),
                IpfsHash([2; 32]),
            ));

            assert_ok!(Content::set_not_revisionable(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));

            assert_noop!(
                Content::publish_revision(
                    RuntimeOrigin::signed(alice()),
                    item_id.clone(),
                    Default::default(),
                    Default::default(),
                    IpfsHash([3; 32]),
                ),
                pallet_content::Error::<Runtime>::ItemNotRevisionable
            );

            assert_noop!(
                Content::set_not_revisionable(RuntimeOrigin::signed(alice()), item_id.clone()),
                pallet_content::Error::<Runtime>::ItemNotRevisionable
            );
        });
    }

    #[test]
    fn content_set_not_retractable_blocks_retraction() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), RETRACTABLE);

            assert_ok!(Content::set_not_retractable(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));

            assert_noop!(
                Content::retract_item(RuntimeOrigin::signed(alice()), item_id.clone()),
                pallet_content::Error::<Runtime>::ItemNotRetractable
            );

            assert_noop!(
                Content::set_not_retractable(RuntimeOrigin::signed(alice()), item_id.clone()),
                pallet_content::Error::<Runtime>::ItemNotRetractable
            );
        });
    }

    #[test]
    fn content_wrong_owner_cannot_revise_or_retract() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE | RETRACTABLE);

            assert_noop!(
                Content::publish_revision(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    Default::default(),
                    Default::default(),
                    IpfsHash([2; 32]),
                ),
                pallet_content::Error::<Runtime>::WrongAccount
            );

            assert_noop!(
                Content::retract_item(RuntimeOrigin::signed(bob()), item_id.clone()),
                pallet_content::Error::<Runtime>::WrongAccount
            );

            assert_noop!(
                Content::set_not_revisionable(RuntimeOrigin::signed(bob()), item_id.clone()),
                pallet_content::Error::<Runtime>::WrongAccount
            );

            assert_noop!(
                Content::set_not_retractable(RuntimeOrigin::signed(bob()), item_id),
                pallet_content::Error::<Runtime>::WrongAccount
            );
        });
    }

    #[test]
    fn content_nonexistent_item_errors() {
        new_test_ext().execute_with(|| {
            let fake_id = ItemId([0xAB; 32]);

            assert_noop!(
                Content::publish_revision(
                    RuntimeOrigin::signed(alice()),
                    fake_id.clone(),
                    Default::default(),
                    Default::default(),
                    IpfsHash::default(),
                ),
                pallet_content::Error::<Runtime>::ItemNotFound
            );

            assert_noop!(
                Content::retract_item(RuntimeOrigin::signed(alice()), fake_id.clone()),
                pallet_content::Error::<Runtime>::ItemNotFound
            );
        });
    }

    #[test]
    fn account_content_duplicate_add_fails() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_ok!(AccountContent::add_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));
            assert_eq!(AccountContent::get_item_count(alice()), 1);

            assert_noop!(
                AccountContent::add_item(RuntimeOrigin::signed(alice()), item_id.clone()),
                pallet_account_content::Error::<Runtime>::ItemAlreadyAdded
            );
            assert_eq!(AccountContent::get_item_count(alice()), 1);
        });
    }

    #[test]
    fn account_content_remove_not_added_fails() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_noop!(
                AccountContent::remove_item(RuntimeOrigin::signed(alice()), item_id),
                pallet_account_content::Error::<Runtime>::ItemNotAdded
            );
        });
    }

    #[test]
    fn account_content_retracted_item_cannot_be_added() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), RETRACTABLE);

            assert_ok!(Content::retract_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));

            assert_noop!(
                AccountContent::add_item(RuntimeOrigin::signed(alice()), item_id.clone()),
                pallet_account_content::Error::<Runtime>::ItemRetracted
            );

            assert_noop!(
                AccountProfile::set_profile(RuntimeOrigin::signed(alice()), item_id),
                pallet_account_profile::Error::<Runtime>::ItemRetracted
            );
        });
    }

    #[test]
    fn account_profile_can_be_overwritten() {
        new_test_ext().execute_with(|| {
            let item_id_1 = publish_item(alice(), nonce_from_byte(0), REVISIONABLE);
            let item_id_2 = publish_item(alice(), nonce_from_byte(1), REVISIONABLE);

            assert_ok!(AccountProfile::set_profile(
                RuntimeOrigin::signed(alice()),
                item_id_1.clone(),
            ));
            assert_eq!(
                pallet_account_profile::AccountProfile::<Runtime>::get(alice()),
                Some(item_id_1.clone())
            );

            assert_ok!(AccountProfile::set_profile(
                RuntimeOrigin::signed(alice()),
                item_id_2.clone(),
            ));
            assert_eq!(
                pallet_account_profile::AccountProfile::<Runtime>::get(alice()),
                Some(item_id_2)
            );
        });
    }

    #[test]
    fn content_reactions_nonexistent_item_and_revision_errors() {
        new_test_ext().execute_with(|| {
            let fake_id = ItemId([0xCC; 32]);

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    fake_id.clone(),
                    0,
                    bounded_vec![Emoji(0x1F600)],
                ),
                pallet_content_reactions::Error::<Runtime>::ItemNotFound
            );

            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    1,
                    bounded_vec![Emoji(0x1F600)],
                ),
                pallet_content_reactions::Error::<Runtime>::RevisionNotFound
            );
        });
    }

    #[test]
    fn content_reactions_invalid_emoji_fails() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    0,
                    bounded_vec![Emoji(0)],
                ),
                pallet_content_reactions::Error::<Runtime>::InvalidEmoji
            );

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    0,
                    bounded_vec![Emoji(0xD800)],
                ),
                pallet_content_reactions::Error::<Runtime>::InvalidEmoji
            );
        });
    }

    #[test]
    fn content_reactions_duplicate_emoji_is_noop() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id.clone(),
                0,
                bounded_vec![Emoji(0x1F600)],
            ));
            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    0,
                    bounded_vec![Emoji(0x1F600), Emoji(0x1F600)],
                ),
                pallet_content_reactions::Error::<Runtime>::DuplicateEmoji
            );
        });
    }

    #[test]
    fn content_reactions_remove_absent_is_noop() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), REVISIONABLE);

            assert_ok!(ContentReactions::set_reactions(
                RuntimeOrigin::signed(bob()),
                item_id.clone(),
                0,
                bounded_vec![],
            ));
        });
    }

    #[test]
    fn content_reactions_retracted_item_errors() {
        new_test_ext().execute_with(|| {
            let item_id = publish_item(alice(), Nonce::default(), RETRACTABLE);

            assert_ok!(Content::retract_item(
                RuntimeOrigin::signed(alice()),
                item_id.clone(),
            ));

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id.clone(),
                    0,
                    bounded_vec![Emoji(0x1F600)],
                ),
                pallet_content_reactions::Error::<Runtime>::ItemRetracted
            );

            assert_noop!(
                ContentReactions::set_reactions(
                    RuntimeOrigin::signed(bob()),
                    item_id,
                    0,
                    bounded_vec![],
                ),
                pallet_content_reactions::Error::<Runtime>::ItemRetracted
            );
        });
    }

    #[test]
    fn utility_batch_dispatch_works() {
        new_test_ext().execute_with(|| {
            let item_id_1 = publish_item(alice(), nonce_from_byte(0), REVISIONABLE);
            let item_id_2 = publish_item(alice(), nonce_from_byte(1), REVISIONABLE);

            let call_1 =
                RuntimeCall::AccountContent(pallet_account_content::Call::<Runtime>::add_item {
                    item_id: item_id_1.clone(),
                });
            let call_2 =
                RuntimeCall::AccountContent(pallet_account_content::Call::<Runtime>::add_item {
                    item_id: item_id_2.clone(),
                });

            assert_ok!(Utility::batch(
                RuntimeOrigin::signed(alice()),
                vec![call_1, call_2],
            ));

            assert_eq!(AccountContent::get_item_count(alice()), 2);
            assert!(AccountContent::get_item_exists(alice(), item_id_1));
            assert!(AccountContent::get_item_exists(alice(), item_id_2));
        });
    }

    #[test]
    fn account_nonce_reflects_free_balance() {
        new_test_ext().execute_with(|| {
            let initial_alice = Balances::free_balance(alice());
            let initial_charlie = Balances::free_balance(charlie());

            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(alice()),
                charlie().into(),
                1000,
            ));
            assert_eq!(Balances::free_balance(alice()), initial_alice - 1000);
            assert_eq!(Balances::free_balance(charlie()), initial_charlie + 1000);

            assert_eq!(System::account_nonce(alice()), 0);
        });
    }

    /// The signed-transaction extension pipeline, as the node builds it.
    fn tx_extension(system_nonce: interface::Nonce) -> TxExtension {
        TxExtension::new((
            frame_system::AuthorizeCall::<Runtime>::new(),
            frame_system::CheckNonZeroSender::<Runtime>::new(),
            frame_system::CheckSpecVersion::<Runtime>::new(),
            frame_system::CheckTxVersion::<Runtime>::new(),
            frame_system::CheckGenesis::<Runtime>::new(),
            frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::Immortal),
            frame_system::CheckNonce::<Runtime>(system_nonce),
            frame_system::CheckWeight::<Runtime>::new(),
            pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
        ))
    }

    /// Sign `call` for Alice exactly the way a client would: the payload is the
    /// encoded `SignedPayload` (call + extension + the extension's implicit
    /// data), blake2-hashed when it exceeds 256 bytes.
    fn signed_extrinsic(
        call: RuntimeCall,
        system_nonce: interface::Nonce,
    ) -> ExtrinsicFor<Runtime> {
        let extension = tx_extension(system_nonce);
        let payload = sp_runtime::generic::SignedPayload::new(call.clone(), extension.clone())
            .expect("extension implicit data is available")
            .encode();
        let payload = if payload.len() > 256 {
            sp_io::hashing::blake2_256(&payload).to_vec()
        } else {
            payload
        };

        let pair = Sr25519Keyring::Alice.pair();
        let signature = sp_core::Pair::sign(&pair, &payload);
        ExtrinsicFor::<Runtime>::new_signed(call, alice().into(), signature.into(), extension)
    }

    fn header(number: u32) -> HeaderFor<Runtime> {
        HeaderFor::<Runtime> {
            parent_hash: Default::default(),
            number,
            state_root: Default::default(),
            extrinsics_root: Default::default(),
            digest: Default::default(),
        }
    }

    /// Hand the parachain its relay-chain context, exactly as the
    /// `set_validation_data` inherent does in a real block. Every parachain
    /// block needs it: `ParachainSystem::on_finalize` panics without it, and it
    /// is what carries the relay parent number and messaging state.
    fn set_validation_data() {
        use cumulus_primitives_core::PersistedValidationData;
        use cumulus_primitives_parachain_inherent::ParachainInherentData;
        use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

        let parent_head = header(System::block_number()).encode();
        let mut sproof_builder = RelayStateSproofBuilder {
            para_id: cumulus_primitives_core::ParaId::new(1000),
            ..Default::default()
        };
        sproof_builder.included_para_head = Some(parent_head.clone().into());
        let (relay_parent_storage_root, relay_chain_state) =
            sproof_builder.into_state_root_and_proof();

        let data = ParachainInherentData {
            validation_data: PersistedValidationData {
                parent_head: parent_head.into(),
                relay_parent_number: 1,
                relay_parent_storage_root,
                max_pov_size: 5 * 1024 * 1024,
            },
            relay_chain_state,
            downward_messages: Default::default(),
            horizontal_messages: Default::default(),
            relay_parent_descendants: Default::default(),
            collator_peer_id: None,
        };

        let mut inherent_data = sp_inherents::InherentData::default();
        inherent_data
            .put_data(
                cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER,
                &data,
            )
            .expect("inherent data fits");

        use frame_support::{inherent::ProvideInherent, traits::UnfilteredDispatchable};

        let call: RuntimeCall = ParachainSystem::create_inherent(&inherent_data)
            .expect("a validation-data inherent is produced")
            .into();
        call.dispatch_bypass_filter(RuntimeOrigin::none())
            .expect("the validation-data inherent dispatches");
    }

    fn transfer_call() -> RuntimeCall {
        RuntimeCall::Balances(pallet_balances::Call::<Runtime>::transfer_allow_death {
            dest: bob().into(),
            value: 1_000,
        })
    }

    #[test]
    fn signed_extrinsic_charges_fee_and_bumps_nonce() {
        new_test_ext().execute_with(|| {
            RuntimeExecutive::initialize_block(&header(System::block_number() + 1));
            set_validation_data();

            let initial_alice = Balances::free_balance(alice());
            let initial_bob = Balances::free_balance(bob());
            let uxt = signed_extrinsic(transfer_call(), 0);
            let len = uxt.encoded_size() as u128;

            assert_eq!(System::account_nonce(alice()), 0);
            let weight_before = System::block_weight().total();
            assert_ok!(RuntimeExecutive::apply_extrinsic(uxt));

            // The nonce advanced and the transfer landed.
            assert_eq!(System::account_nonce(alice()), 1);
            assert_eq!(Balances::free_balance(bob()), initial_bob + 1_000);

            // Fees are charged on both weight and length.
            let paid = initial_alice - Balances::free_balance(alice()) - 1_000;
            assert!(paid > 0, "a signed extrinsic must not be free");
            let length_only = TransactionPayment::length_to_fee(len as u32);
            assert!(
                paid > length_only,
                "expected a weight component on top of the {length_only} length fee, paid {paid}"
            );

            // The charged weight is the extension's own weight, plus the call,
            // plus the per-extrinsic base weight. Only initialization weight was
            // accounted for before `weight_before`.
            let extension_weight = sp_runtime::traits::TransactionExtension::weight(
                &tx_extension(0),
                &transfer_call(),
            );
            let call_weight = transfer_call().get_dispatch_info().call_weight;
            let base_extrinsic =
                <<Runtime as frame_system::Config>::BlockWeights as frame_support::traits::Get<
                    frame_system::limits::BlockWeights,
                >>::get()
                .get(frame_support::dispatch::DispatchClass::Normal)
                .base_extrinsic;
            assert!(extension_weight.ref_time() > 0);
            // Ref time is exact; proof size tracks the actual storage proof of the
            // accesses made (which can differ slightly from the declared estimate).
            let charged = extension_weight
                .saturating_add(call_weight)
                .saturating_add(base_extrinsic);
            assert_eq!(
                System::block_weight().total().ref_time(),
                weight_before.ref_time() + charged.ref_time()
            );

            // A real block carries the timestamp inherent; `on_finalize` panics
            // if it was never set. The value has to fall inside the current Aura
            // slot, and no pre-runtime seal digest was provided, so the slot is
            // still 0 (`MILLI_SECS_PER_BLOCK / 2` is inside slot 0).
            assert_ok!(Timestamp::set(
                RuntimeOrigin::none(),
                MILLI_SECS_PER_BLOCK / 2
            ));
            RuntimeExecutive::finalize_block();

            let events = System::events();
            assert!(events.iter().any(|record| matches!(
                &record.event,
                RuntimeEvent::Balances(pallet_balances::Event::Transfer {
                    from,
                    to,
                    amount,
                }) if from == &alice() && to == &bob() && amount == &1_000
            )));
            assert!(events.iter().any(|record| matches!(
                &record.event,
                RuntimeEvent::TransactionPayment(
                    pallet_transaction_payment::Event::TransactionFeePaid { who, .. }
                ) if who == &alice()
            )));
        });
    }

    #[test]
    fn signed_extrinsic_with_wrong_nonce_is_rejected() {
        new_test_ext().execute_with(|| {
            RuntimeExecutive::initialize_block(&header(System::block_number() + 1));
            set_validation_data();

            let initial_alice = Balances::free_balance(alice());

            // Signed with a future nonce: rejected before dispatch, so no state
            // change and no fee.
            let err = RuntimeExecutive::apply_extrinsic(signed_extrinsic(transfer_call(), 5))
                .expect_err("a future nonce must not be accepted");
            assert_eq!(
                err,
                sp_runtime::transaction_validity::TransactionValidityError::Invalid(
                    sp_runtime::transaction_validity::InvalidTransaction::Future
                )
            );
            assert_eq!(System::account_nonce(alice()), 0);
            assert_eq!(Balances::free_balance(alice()), initial_alice);

            // A stale nonce is rejected the same way once the nonce moves on.
            assert_ok!(RuntimeExecutive::apply_extrinsic(signed_extrinsic(
                transfer_call(),
                0
            )));
            assert_eq!(System::account_nonce(alice()), 1);
            let err = RuntimeExecutive::apply_extrinsic(signed_extrinsic(transfer_call(), 0))
                .expect_err("a replayed nonce must not be accepted");
            assert_eq!(
                err,
                sp_runtime::transaction_validity::TransactionValidityError::Invalid(
                    sp_runtime::transaction_validity::InvalidTransaction::Stale
                )
            );
        });
    }

    /// CHOR's supply is fixed: the only calls that can change `TotalIssuance`
    /// are root-only, and this runtime refuses them for every non-root origin —
    /// including when laundered through sudo, a batch or the scheduler.
    #[test]
    fn supply_guard_rejects_issuance_calls_from_non_root() {
        use pallet_balances::AdjustmentDirection;
        use sp_runtime::traits::Dispatchable;

        new_test_ext().execute_with(|| {
            fn issuance_call(delta: u128) -> RuntimeCall {
                RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance {
                    direction: AdjustmentDirection::Increase,
                    delta,
                })
            }

            let issuance_before = pallet_balances::TotalIssuance::<Runtime>::get();

            // (1) A plain signed caller is rejected.
            assert!(
                issuance_call(1_000)
                    .dispatch(RuntimeOrigin::signed(alice()))
                    .is_err(),
                "a signed account must not change issuance"
            );

            // (2) The sudo key must not be able to launder it. `Sudo::sudo`
            // dispatches its inner call as Root, so without the guard this
            // would succeed; the guard inspects the wrapper.
            let sudo_wrapped = RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: alloc::boxed::Box::new(issuance_call(1_000)),
            });
            assert!(
                sudo_wrapped
                    .dispatch(RuntimeOrigin::signed(alice()))
                    .is_err(),
                "sudo must not be able to change issuance"
            );

            // (3) Nor through a batch.
            let batched = RuntimeCall::Utility(pallet_utility::Call::batch {
                calls: vec![issuance_call(1_000)],
            });
            assert!(
                batched.dispatch(RuntimeOrigin::signed(alice())).is_err(),
                "a batch must not launder an issuance call"
            );

            assert_eq!(
                pallet_balances::TotalIssuance::<Runtime>::get(),
                issuance_before,
                "no non-root path may have moved issuance"
            );

            // (4) Root still reaches it: this is the documented residual — root
            // means the `root` referendum track (or a runtime upgrade), both
            // public and slow. That it succeeds here is what proves (2) is the
            // guard working rather than the pallet's own `ensure_root`.
            assert!(
                issuance_call(1_000)
                    .dispatch(RuntimeOrigin::root())
                    .is_ok(),
                "root must retain the root-only path"
            );
            assert_eq!(
                pallet_balances::TotalIssuance::<Runtime>::get(),
                issuance_before + 1_000,
                "root issuance change should have applied"
            );
        });
    }

    /// The guard is targeted: ordinary calls pass, and root keeps the
    /// root-only paths that a `root` referendum legitimately needs.
    #[test]
    fn supply_guard_allows_ordinary_calls_and_root_paths() {
        new_test_ext().execute_with(|| {
            assert!(supply_guard::SupplyGuard::contains(&transfer_call()));

            let issuance_call = RuntimeCall::Balances(
                pallet_balances::Call::force_adjust_total_issuance {
                    direction: pallet_balances::AdjustmentDirection::Increase,
                    delta: 1,
                },
            );
            // Not *rejected by* the guard — root can still reach it, which is
            // the documented residual: root == the `root` referendum track.
            assert!(!supply_guard::SupplyGuard::contains(&issuance_call));

            // But a sudo-wrapped version is recognised as supply-changing.
            let wrapped = RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: alloc::boxed::Box::new(issuance_call),
            });
            assert!(!supply_guard::SupplyGuard::contains(&wrapped));
        });
    }
}
