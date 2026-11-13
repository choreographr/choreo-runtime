//! Runtime parameters that governance can change without a runtime upgrade.
//!
//! `pallet_parameters` stores a key/value map keyed by the generated
//! `RuntimeParametersKey`. Each `static` below becomes one key, and the
//! `Get<…>` type it generates is used directly by the pallet that reads it — so
//! the value in storage, not the constant in code, is what applies.

use polkadot_sdk::frame_support::dynamic_params::{dynamic_pallet_params, dynamic_params};
use polkadot_sdk::*;

use crate::{AccountId, Runtime, RuntimeEvent, RuntimeOrigin};

/// The parameters that may be adjusted at runtime.
///
/// Defaults here are the values used until governance changes them. The
/// scheduler default mirrors `MaximumSchedulerWeight`: 80% of a two-second
/// block, with the matching proof-size budget.
#[dynamic_params(RuntimeParameters, pallet_parameters::Parameters::<Runtime>)]
pub mod dynamic_params {
    use super::*;

    #[dynamic_pallet_params]
    #[codec(index = 0)]
    pub mod scheduler {
        use super::*;
        /// Maximum weight a single scheduled call may use.
        #[codec(index = 0)]
        pub static MaximumWeight: frame_support::weights::Weight =
            frame_support::weights::Weight::from_parts(1_600_000_000_000, 4_194_304);
        /// How many scheduled calls may be dispatched per block.
        #[codec(index = 1)]
        pub static MaxScheduledPerBlock: u32 = 50;
    }
}

/// Who may change a parameter: the `root` track, or the `general_admin` track
/// for routine tuning. Once sudo is removed this is governance-only.
pub struct DynamicParameterOrigin;
impl frame_support::traits::EnsureOriginWithArg<RuntimeOrigin, RuntimeParametersKey>
    for DynamicParameterOrigin
{
    type Success = ();

    fn try_origin(
        origin: RuntimeOrigin,
        _key: &RuntimeParametersKey,
    ) -> Result<Self::Success, RuntimeOrigin> {
        // Root (the `root` track) or the `general_admin` track may retune.
        if <frame_system::EnsureRoot<AccountId> as frame_support::traits::EnsureOrigin<
            RuntimeOrigin,
        >>::try_origin(origin.clone())
        .is_ok()
        {
            return Ok(());
        }
        if <crate::governance::origins::GeneralAdmin as frame_support::traits::EnsureOrigin<
            RuntimeOrigin,
        >>::try_origin(origin.clone())
        .is_ok()
        {
            return Ok(());
        }
        Err(origin)
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
        Ok(RuntimeOrigin::root())
    }
}

impl pallet_parameters::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeParameters = RuntimeParameters;
    type AdminOrigin = DynamicParameterOrigin;
    type WeightInfo = ();
}
