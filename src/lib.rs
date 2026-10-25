#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

use alloc::vec::Vec;
use frame_support::weights::ConstantMultiplier;
use pallet_transaction_payment::{FeeDetails, RuntimeDispatchInfo};
use polkadot_sdk::{
    polkadot_sdk_frame::{
        self as frame,
        deps::sp_genesis_builder,
        runtime::{apis, prelude::*},
    },
    staging_parachain_info as parachain_info, *,
};
use sp_session::OpaqueGeneratedSessionKeys;

pub mod genesis_config_presets {
    use super::*;
    use crate::{BalancesConfig, ParachainInfoConfig, RuntimeGenesisConfig, SudoConfig};
    use alloc::vec;
    use cumulus_primitives_core::ParaId;
    use serde_json::Value;
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_keyring::Sr25519Keyring;

    fn dev_authority() -> AuraId {
        Sr25519Keyring::Alice.public().into()
    }

    pub fn development_config_genesis() -> Value {
        frame_support::build_struct_json_patch!(RuntimeGenesisConfig {
            balances: BalancesConfig {
                balances: vec![
                    (Sr25519Keyring::Alice.to_account_id(), 1_000_000_000_000_000),
                    (Sr25519Keyring::Bob.to_account_id(), 1_000_000_000_000_000),
                ]
            },
            aura: pallet_aura::GenesisConfig {
                authorities: vec![dev_authority()],
            },
            parachain_info: ParachainInfoConfig {
                parachain_id: ParaId::new(1000),
            },
            sudo: SudoConfig {
                key: Some(Sr25519Keyring::Alice.to_account_id()),
            },
        })
    }

    pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
        let patch = match id.as_ref() {
            sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
            _ => return None,
        };

        Some(serde_json::to_string(&patch).ok()?.into_bytes())
    }

    pub fn preset_names() -> Vec<PresetId> {
        alloc::vec![PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET)]
    }
}

pub mod weights;

#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("acuity-runtime"),
    impl_name: alloc::borrow::Cow::Borrowed("acuity-runtime"),
    authoring_version: 1,
    spec_version: 1,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};

type TxExtension = (
    frame_system::AuthorizeCall<Runtime>,
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    frame_system::WeightReclaim<Runtime>,
);

const MILLI_SECS_PER_BLOCK: u64 = 6000;

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
}

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;
    pub const ItemIdNamespace: u32 = 1000;
    pub const MaxParents: u32 = 32;
    pub const MaxLinks: u32 = 128;
    pub const MaxMentions: u32 = 256;
    pub const MaxItemsPerAccount: u32 = 1024;
    pub const MaxEmojis: u32 = 16;
    /// Messages from the relay chain are dropped for now: no DMP queue is
    /// wired, so this reserves no block weight for processing them.
    pub const ReservedDmpWeight: Weight = Weight::zero();
    /// Same for XCMP: there is no XCMP queue pallet, so nothing is reserved.
    pub const ReservedXcmpWeight: Weight = Weight::zero();
}

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type Version = Version;
    type AccountData = pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
    type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnSystemEvent = ();
    type SelfParaId = ParachainInfo;
    type OutboundXcmpMessageSource = ();
    type DmpQueue = ();
    type ReservedDmpWeight = ReservedDmpWeight;
    type XcmpMessageHandler = ();
    type ReservedXcmpWeight = ReservedXcmpWeight;
    type CheckAssociatedRelayNumber =
        cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
    type ConsensusHook = cumulus_pallet_parachain_system::consensus_hook::RequireParentIncluded;
    type WeightInfo = ();
    type RelayParentOffset = ConstU32<0>;
    type SchedulingSignatureVerifier = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
    type Balance = u128;
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

#[derive_impl(pallet_sudo::config_preludes::TestDefaultConfig)]
impl pallet_sudo::Config for Runtime {}

impl pallet_timestamp::Config for Runtime {
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<{ MILLI_SECS_PER_BLOCK / 2 }>;
    type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Runtime {
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
    type WeightToFee = NoFee<<Self as pallet_balances::Config>::Balance>;
    type LengthToFee = ConstantMultiplier<<Self as pallet_balances::Config>::Balance, ConstU128<1>>;
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

#[cfg(feature = "runtime-benchmarks")]
impl frame_system_benchmarking::Config for Runtime {}

type Block = frame::runtime::types_common::BlockOf<Runtime, TxExtension>;
type Header = HeaderFor<Runtime>;
type RuntimeExecutive =
    Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllPalletsWithSystem>;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    use super::*;
    use frame_support::traits::{StorageInfo, WhitelistedStorageKeys};
    use polkadot_sdk::frame_system_benchmarking::Pallet as SystemBench;

    polkadot_sdk::frame_benchmarking::define_benchmarks!(
        [frame_system, SystemBench::<Runtime>]
        [pallet_account_content, AccountContent]
        [pallet_account_profile, AccountProfile]
        [pallet_balances, Balances]
        [pallet_content, Content]
        [pallet_content_reactions, ContentReactions]
        [pallet_utility, Utility]
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
        fn generate_session_keys(_: Vec<u8>, _: Option<Vec<u8>>) -> OpaqueGeneratedSessionKeys {
            Default::default()
        }

        fn decode_session_keys(_: Vec<u8>) -> Option<Vec<(Vec<u8>, apis::KeyTypeId)>> {
            Default::default()
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

        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig,
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
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
                    (alice(), 1_000_000_000_000_000),
                    (bob(), 1_000_000_000_000_000),
                    (charlie(), 1_000_000_000_000_000),
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
                    Value::Number(1_000_000_000_000_000u64.into()),
                ]),
                Value::Array(vec![
                    Value::String(bob().to_ss58check()),
                    Value::Number(1_000_000_000_000_000u64.into()),
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
        assert_eq!(VERSION.spec_name.as_ref(), "acuity-runtime");
        assert_eq!(VERSION.impl_name.as_ref(), "acuity-runtime");
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
    fn transaction_payment_uses_zero_weight_fee_and_linear_length_fee() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                TransactionPayment::weight_to_fee(Weight::from_parts(999, 0)),
                0
            );
            assert_eq!(TransactionPayment::length_to_fee(1), 1);
            assert_eq!(TransactionPayment::length_to_fee(7), 7);
            assert_eq!(TransactionPayment::length_to_fee(1024), 1024);
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
            assert_eq!(initial, 1_000_000_000_000_000);

            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(alice()),
                bob().into(),
                500,
            ));
            assert_eq!(Balances::free_balance(alice()), initial - 500);
            assert_eq!(Balances::free_balance(bob()), 1_000_000_000_000_000 + 500);

            assert_eq!(Balances::free_balance(charlie()), 1_000_000_000_000_000);
            assert_noop!(
                Balances::transfer_allow_death(
                    RuntimeOrigin::signed(charlie()),
                    alice().into(),
                    1_000_000_000_000_001,
                ),
                DispatchError::Token(TokenError::FundsUnavailable)
            );

            let new_account: AccountId = sp_runtime::AccountId32::from([0x42u8; 32]);
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(alice()),
                new_account.clone().into(),
                1,
            ));
            assert_eq!(Balances::free_balance(new_account), 1);
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
        (
            frame_system::AuthorizeCall::<Runtime>::new(),
            frame_system::CheckNonZeroSender::<Runtime>::new(),
            frame_system::CheckSpecVersion::<Runtime>::new(),
            frame_system::CheckTxVersion::<Runtime>::new(),
            frame_system::CheckGenesis::<Runtime>::new(),
            frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::Immortal),
            frame_system::CheckNonce::<Runtime>(system_nonce),
            frame_system::CheckWeight::<Runtime>::new(),
            pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
            frame_system::WeightReclaim::<Runtime>::new(),
        )
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

            // Weight fee is zero and the length fee is one unit per byte, so the
            // only thing Alice pays beyond the transfer is the encoded length.
            let paid = initial_alice - Balances::free_balance(alice()) - 1_000;
            assert_eq!(paid, len, "expected a length-only fee of {len}");
            assert!(paid > 0, "a signed extrinsic must not be free");

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
}
