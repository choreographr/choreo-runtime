//! Supply-authority closure.
//!
//! CHOR's issuance is fixed by genesis: `pallet_balances` has no user-reachable
//! issuance path, so the only calls that can change `TotalIssuance` are the
//! root-only ones — `force_set_balance` and `force_adjust_total_issuance`.
//!
//! This filter closes those off for everything except a genuine `Root` origin:
//!
//! * A signed caller cannot invoke them (`CallFiltered`).
//! * The sudo account cannot invoke them *through* sudo. `Sudo::sudo` executes
//!   its inner call with `Root`, which would bypass any call filter, so the
//!   wrapper is unwrapped and inspected here. Same for `Utility` batches and
//!   `Scheduler` scheduling, which would otherwise launder the call.
//!
//! What remains, deliberately:
//!
//! * the `root` referendum track — 14-day decision period, 80% approval, one
//!   referendum deciding at a time — and
//! * a runtime upgrade (`System::set_code`).
//!
//! Both are public, slow and visible. There is no quiet mint.

use polkadot_sdk::*;

use crate::RuntimeCall;
use frame_support::traits::Contains;

pub struct SupplyGuard;

impl Contains<RuntimeCall> for SupplyGuard {
    fn contains(call: &RuntimeCall) -> bool {
        !is_supply_changing(call)
    }
}

/// True if this call — including when laundered through an origin-changing
/// wrapper — can change the total issuance.
fn is_supply_changing(call: &RuntimeCall) -> bool {
    match call {
        RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }) => true,
        RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance { .. }) => true,

        // Dispenses the inner call with `Root`: must be inspected, not trusted.
        RuntimeCall::Sudo(pallet_sudo::Call::sudo { call }) => is_supply_changing(call),
        RuntimeCall::Sudo(pallet_sudo::Call::sudo_unchecked_weight { call, .. }) => {
            is_supply_changing(call)
        }

        // Batches dispatch with the same origin, so the filter would catch the
        // inner call anyway; unwrapping keeps this correct if that changes and
        // documents the intent.
        RuntimeCall::Utility(pallet_utility::Call::batch { calls })
        | RuntimeCall::Utility(pallet_utility::Call::batch_all { calls })
        | RuntimeCall::Utility(pallet_utility::Call::force_batch { calls }) => {
            calls.iter().any(is_supply_changing)
        },
        RuntimeCall::Utility(pallet_utility::Call::as_derivative { call, .. })
        | RuntimeCall::Utility(pallet_utility::Call::dispatch_as { call, .. })
        | RuntimeCall::Utility(pallet_utility::Call::with_weight { call, .. }) => {
            is_supply_changing(call)
        }

        // A root-scheduled call executes later with `Root`; catch it at the
        // point it is scheduled.
        RuntimeCall::Scheduler(pallet_scheduler::Call::schedule { call, .. })
        | RuntimeCall::Scheduler(pallet_scheduler::Call::schedule_named { call, .. })
        | RuntimeCall::Scheduler(pallet_scheduler::Call::schedule_after { call, .. })
        | RuntimeCall::Scheduler(pallet_scheduler::Call::schedule_named_after { call, .. }) => {
            is_supply_changing(call)
        }

        _ => false,
    }
}
