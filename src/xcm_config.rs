//! Minimal-viable XCM: HRMP to siblings (Hydration), UMP/DMP to the relay chain,
//! and reserve transfers of the native CHOR asset.
//!
//! Deliberately *not* enabled yet: teleports, local `pallet_xcm::execute`
//! dispatch, asset conversion as a fee mechanism, and foreign assets. Those are
//! additive and can land later without touching this layout.

use polkadot_sdk::{
    polkadot_sdk_frame::traits::Disabled,
    staging_xcm as xcm,
    staging_xcm_builder as xcm_builder,
    staging_xcm_executor as xcm_executor,
    *,
};

use crate::{
    governance::origins, AccountId, AllPalletsWithSystem, Balances, ParachainInfo, ParachainSystem,
    PolkadotXcm, PriceForParentDelivery, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
    WeightToFee, XcmpQueue,
};

use frame_support::{
    parameter_types,
    traits::{ConstU32, Contains, Everything, Nothing},
    weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use polkadot_parachain_primitives::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use staging_xcm_builder::{DenyRecursively, DenyThenTry};
use xcm::latest::prelude::*;
use xcm_builder::{
    AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowTopLevelPaidExecutionFrom,
    DenyReserveTransferToRelayChain, EnsureXcmOrigin, FixedWeightBounds, FrameTransactionalProcessor,
    FungibleAdapter, IsConcrete, NativeAsset, ParentIsPreset, RelayChainAsNative,
    SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
    SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit, TrailingSetTopicAsId,
    UsingComponents, WithComputedOrigin, WithUniqueTopic,
};
use xcm_executor::XcmExecutor;

parameter_types! {
    pub const RelayLocation: Location = Location::parent();
    pub const RelayNetwork: Option<NetworkId> = None;
    pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
    /// The universal location of this parachain. `GlobalConsensus` is
    /// intentionally omitted: set it (and `RelayNetwork`) once the chain is
    /// pinned to a specific relay chain, which also makes location matching
    /// stricter.
    pub UniversalLocation: InteriorLocation = Parachain(ParachainInfo::parachain_id().into()).into();
}

/// How a `Location` becomes an `AccountId`: used for sovereign accounts and for
/// resolving `Transact` origins.
pub type LocationToAccountId = (
    ParentIsPreset<AccountId>,
    SiblingParachainConvertsVia<Sibling, AccountId>,
    AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting the native CHOR asset here.
pub type LocalAssetTransactor = FungibleAdapter<
    Balances,
    IsConcrete<RelayLocation>,
    LocationToAccountId,
    AccountId,
    (),
>;

/// How an incoming XCM origin becomes a local dispatch origin.
pub type XcmOriginToTransactDispatchOrigin = (
    SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
    RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
    SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
    SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
    XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
    pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
    pub const MaxInstructions: u32 = 100;
    pub const MaxAssetsIntoHolding: u32 = 64;
}

/// Parent (and its executive plurality) get free execution; nobody else.
pub struct ParentOrParentsExecutivePlurality;
impl Contains<Location> for ParentOrParentsExecutivePlurality {
    fn contains(location: &Location) -> bool {
        matches!(
            location.unpack(),
            (1, []) | (1, [Plurality { id: BodyId::Executive, .. }])
        )
    }
}

pub type Barrier = TrailingSetTopicAsId<
    DenyThenTry<
        DenyRecursively<DenyReserveTransferToRelayChain>,
        (
            TakeWeightCredit,
            WithComputedOrigin<
                (
                    AllowTopLevelPaidExecutionFrom<Everything>,
                    AllowExplicitUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
                ),
                UniversalLocation,
                ConstU32<8>,
            >,
        ),
    >,
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
    type RuntimeCall = RuntimeCall;
    type XcmSender = XcmRouter;
    type XcmEventEmitter = PolkadotXcm;
    type AssetTransactor = LocalAssetTransactor;
    type OriginConverter = XcmOriginToTransactDispatchOrigin;
    /// We are the reserve for native CHOR: sibling chains holding CHOR treat
    /// this chain as its reserve.
    type IsReserve = NativeAsset;
    type IsTeleporter = ();
    type UniversalLocation = UniversalLocation;
    type Barrier = Barrier;
    type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
    type Trader = UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
    type ResponseHandler = PolkadotXcm;
    type AssetTrap = PolkadotXcm;
    type SubscriptionService = PolkadotXcm;
    type PalletInstancesInfo = AllPalletsWithSystem;
    type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
    type AssetLocker = ();
    type AssetExchanger = ();
    type FeeManager = ();
    type MessageExporter = ();
    type UniversalAliases = Nothing;
    type CallDispatcher = RuntimeCall;
    /// Security-critical: only allow calls that are safe to be dispatched by a
    /// remote XCM message. Tighten before launch if `Transact` is exposed.
    type SafeCallFilter = Nothing;
    type Aliasers = Nothing;
    type TransactionalProcessor = FrameTransactionalProcessor;
    type HrmpNewChannelOpenRequestHandler = ();
    type HrmpChannelAcceptedHandler = ();
    type HrmpChannelClosingHandler = ();
    type XcmRecorder = PolkadotXcm;
}

/// Local signed origin -> XCM location.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// Router: UMP to the relay chain, XCMP to siblings.
pub type XcmRouter = WithUniqueTopic<(
    cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>,
    XcmpQueue,
)>;

/// Root or the `general_admin` track may administer XCM (version subscriptions,
/// asset registration, and so on).
pub type XcmAdminOrigin = EitherOfDiverse<EnsureRoot<AccountId>, origins::GeneralAdmin>;

use frame_support::traits::EitherOfDiverse;

impl pallet_xcm::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    type XcmRouter = XcmRouter;
    type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    /// No local `pallet_xcm::execute`: XCM is only dispatched by the queues.
    type XcmExecuteFilter = Nothing;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    /// Teleports are disabled; we are not a teleporter.
    type XcmTeleportFilter = Nothing;
    /// Reserve transfers are enabled so users (and later the treasury) can move
    /// CHOR to Hydration and other siblings.
    type XcmReserveTransferFilter = Everything;
    type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
    type UniversalLocation = UniversalLocation;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
    type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
    type Currency = Balances;
    type CurrencyMatcher = ();
    type TrustedLockers = ();
    type SovereignAccountOf = LocationToAccountId;
    type MaxLockers = ConstU32<8>;
    type MaxRemoteLockConsumers = ConstU32<0>;
    type RemoteLockConsumerIdentifier = ();
    type WeightInfo = pallet_xcm::TestWeightInfo;
    type AdminOrigin = XcmAdminOrigin;
    type AuthorizedAliasConsideration = Disabled;
}

impl cumulus_pallet_xcm::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type XcmExecutor = XcmExecutor<XcmConfig>;
}
