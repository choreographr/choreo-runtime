//! The OpenGov governance stack: referenda, conviction voting, whitelisting,
//! preimages, scheduling and the treasury.
//!
//! `pallet_parameters` is wired separately (see `runtime::parameters`) because it
//! needs the `dynamic_params` key/value aggregation.

use polkadot_sdk::*;

use crate::{
    governance::origins::{Spender, WhitelistedCaller},
    AccountId, AccountIdConversion, Balance, Balances, BlockNumber, ConstU32, IdentityLookup,
    OriginCaller, Preimage, Referenda, Runtime, RuntimeBlockWeights, RuntimeCall, RuntimeEvent,
    RuntimeHoldReason, RuntimeOrigin, Scheduler, System, Treasury, DAYS, HOURS, MILLI_UNIT,
    MINUTES, UNITS,
};
use frame_support::{
    parameter_types,
    traits::{
        fungible::HoldConsideration, EitherOf, EqualPrivilegeOnly, LinearStoragePrice,
    },
    PalletId,
};
use sp_runtime::{Perbill, Permill};
use frame_system::{EnsureRoot, EnsureRootWithSuccess, EnsureSigned};

pub mod origins;
pub mod tracks;

pub use origins::custom_origins;
pub use tracks::TracksInfo;

impl custom_origins::Config for Runtime {}

parameter_types! {
    pub const VoteLockingPeriod: BlockNumber = DAYS;
}

// --- preimage -----------------------------------------------------------------

parameter_types! {
    pub const PreimageBaseDeposit: Balance = 5 * UNITS;
    pub const PreimageByteDeposit: Balance = MILLI_UNIT;
    pub const PreimageHoldReason: RuntimeHoldReason =
        RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
}

impl pallet_preimage::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ManagerOrigin = EnsureRoot<AccountId>;
    type Consideration = HoldConsideration<
        AccountId,
        Balances,
        PreimageHoldReason,
        LinearStoragePrice<PreimageBaseDeposit, PreimageByteDeposit, Balance>,
    >;
    type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
}

// --- conviction voting --------------------------------------------------------

impl pallet_conviction_voting::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type VoteLockingPeriod = VoteLockingPeriod;
    type MaxVotes = ConstU32<512>;
    type MaxTurnout =
        frame_support::traits::tokens::currency::ActiveIssuanceOf<Balances, Self::AccountId>;
    type Polls = Referenda;
    type BlockNumberProvider = System;
    type VotingHooks = ();
    type WeightInfo = pallet_conviction_voting::weights::SubstrateWeight<Runtime>;
}

// --- scheduler ----------------------------------------------------------------

parameter_types! {
    pub MaximumSchedulerWeight: frame_support::weights::Weight =
        Perbill::from_percent(80) * RuntimeBlockWeights::get().max_block;
    pub const MaxScheduledPerBlock: u32 = 50;
    pub const NoPreimagePostponement: Option<BlockNumber> = Some(10);
}

impl pallet_scheduler::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type PalletsOrigin = OriginCaller;
    type RuntimeCall = RuntimeCall;
    type MaximumWeight = MaximumSchedulerWeight;
    type ScheduleOrigin = EnsureRoot<AccountId>;
    type MaxScheduledPerBlock = MaxScheduledPerBlock;
    type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
    type OriginPrivilegeCmp = EqualPrivilegeOnly;
    type Preimages = Preimage;
    type BlockNumberProvider = System;
}

// --- whitelist ----------------------------------------------------------------

parameter_types! {
    pub const MaxBalance: Balance = Balance::MAX;
}

/// Treasury spending: root may spend anything, the `treasury_spend` track is
/// capped per call by `origins::Spender`.
pub type TreasurySpender = EitherOf<EnsureRootWithSuccess<AccountId, MaxBalance>, Spender>;

impl pallet_whitelist::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    // Root (and therefore the root track) may whitelist a call for the
    // expedited `whitelisted_caller` track. Once sudo is removed this is
    // governance-only.
    type WhitelistOrigin = EnsureRoot<AccountId>;
    type DispatchWhitelistedOrigin = EitherOf<EnsureRoot<AccountId>, WhitelistedCaller>;
    type Preimages = Preimage;
    type DeferredDispatchExpiration = ConstU32<100>;
    type BlockNumberProvider = System;
    type WeightInfo = pallet_whitelist::weights::SubstrateWeight<Runtime>;
}

// --- referenda ----------------------------------------------------------------

parameter_types! {
    pub const AlarmInterval: BlockNumber = 1;
    pub const SubmissionDeposit: Balance = 3 * MILLI_UNIT;
    pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

impl pallet_referenda::Config for Runtime {
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_referenda::weights::SubstrateWeight<Runtime>;
    type Scheduler = Scheduler;
    type Currency = Balances;
    type SubmitOrigin = EnsureSigned<AccountId>;
    type CancelOrigin = EitherOf<EnsureRoot<AccountId>, origins::ReferendumCanceller>;
    type KillOrigin = EitherOf<EnsureRoot<AccountId>, origins::ReferendumKiller>;
    type Slash = Treasury;
    type Votes = pallet_conviction_voting::VotesOf<Runtime>;
    type Tally = pallet_conviction_voting::TallyOf<Runtime>;
    type SubmissionDeposit = SubmissionDeposit;
    type MaxQueued = ConstU32<100>;
    type UndecidingTimeout = UndecidingTimeout;
    type AlarmInterval = AlarmInterval;
    type Tracks = TracksInfo;
    type Preimages = Preimage;
    type BlockNumberProvider = System;
}

// --- treasury -----------------------------------------------------------------

parameter_types! {
    pub const TreasuryPalletId: PalletId = PalletId(*b"chor/trs");
    pub const TreasurySpendPeriod: BlockNumber = 7 * DAYS;
    /// Percentage of the treasury balance that is burned each spend period.
    pub const Burn: Permill = Permill::from_percent(0);
    pub const MaxApprovals: u32 = 100;
    pub const TreasuryPayoutPeriod: BlockNumber = 30 * DAYS;
    pub TreasuryAccount: AccountId = PalletId(*b"chor/trs").into_account_truncating();
}

impl pallet_treasury::Config for Runtime {
    type Currency = Balances;
    type RejectOrigin = EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = TreasurySpendPeriod;
    type Burn = Burn;
    type PalletId = TreasuryPalletId;
    type BurnDestination = ();
    type SpendFunds = ();
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = TreasurySpender;
    /// Native CHOR only; no foreign-asset spends yet.
    type AssetKind = ();
    type Beneficiary = AccountId;
    type BeneficiaryLookup = IdentityLookup<AccountId>;
    type Paymaster = crate::pay::PayFromAccount<Balances, TreasuryAccount>;
    type BalanceConverter = frame_support::traits::tokens::UnityAssetBalanceConversion;
    type PayoutPeriod = TreasuryPayoutPeriod;
    type BlockNumberProvider = System;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
    type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
}

// Silence "unused import" when the alias block above is trimmed by cfg.
const _: Option<fn()> = None::<fn()>;
