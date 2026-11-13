//! OpenGov referendum tracks.
//!
//! Track ids are stable: they are part of the on-chain ABI (referenda are
//! submitted by track id) and the same numbering is used by `track_for`, which
//! maps a dispatch origin back to the track that may produce it.

use super::*;
use alloc::borrow::Cow;
use pallet_referenda::Curve;
use sp_runtime::str_array as s;

const fn percent(x: i32) -> sp_runtime::FixedI64 {
    sp_runtime::FixedI64::from_rational(x as u128, 100)
}

const TRACKS_DATA: [pallet_referenda::Track<u16, Balance, BlockNumber>; 6] = [
    // 0 — root: the most powerful track. Very high approval and support, a long
    // decision period, and only one referendum may decide at a time.
    pallet_referenda::Track {
        id: 0,
        info: pallet_referenda::TrackInfo {
            name: s("root"),
            max_deciding: 1,
            decision_deposit: 5_000 * UNITS,
            prepare_period: 2 * HOURS,
            decision_period: 14 * DAYS,
            confirm_period: 24 * HOURS,
            min_enactment_period: 20 * MINUTES,
            min_approval: Curve::make_reciprocal(4, 7, percent(80), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 7, percent(20), percent(5), percent(50)),
        },
    },
    // 1 — whitelisted_caller: expedited execution of a call that the technical
    // origin has whitelisted. This is how fast fixes are shipped.
    pallet_referenda::Track {
        id: 1,
        info: pallet_referenda::TrackInfo {
            name: s("whitelisted_caller"),
            max_deciding: 3,
            decision_deposit: 2_000 * UNITS,
            prepare_period: 20 * MINUTES,
            decision_period: 2 * DAYS,
            confirm_period: 2 * HOURS,
            min_enactment_period: 6 * MINUTES,
            min_approval: Curve::make_reciprocal(1, 1, percent(60), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 1, percent(10), percent(5), percent(50)),
        },
    },
    // 2 — referendum_canceller: cancel an in-flight referendum.
    pallet_referenda::Track {
        id: 2,
        info: pallet_referenda::TrackInfo {
            name: s("referendum_canceller"),
            max_deciding: 3,
            decision_deposit: 500 * UNITS,
            prepare_period: 2 * HOURS,
            decision_period: 6 * DAYS,
            confirm_period: 2 * HOURS,
            min_enactment_period: 6 * MINUTES,
            min_approval: Curve::make_reciprocal(1, 3, percent(70), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 3, percent(15), percent(5), percent(50)),
        },
    },
    // 3 — referendum_killer: kill an in-flight referendum outright.
    pallet_referenda::Track {
        id: 3,
        info: pallet_referenda::TrackInfo {
            name: s("referendum_killer"),
            max_deciding: 3,
            decision_deposit: 500 * UNITS,
            prepare_period: 2 * HOURS,
            decision_period: 6 * DAYS,
            confirm_period: 2 * HOURS,
            min_enactment_period: 20 * MINUTES,
            min_approval: Curve::make_reciprocal(1, 3, percent(70), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 3, percent(15), percent(5), percent(50)),
        },
    },
    // 4 — treasury_spend: spend from the treasury, capped per call by
    // `origins::Spender`.
    pallet_referenda::Track {
        id: 4,
        info: pallet_referenda::TrackInfo {
            name: s("treasury_spend"),
            max_deciding: 3,
            decision_deposit: 5_000 * UNITS,
            prepare_period: 2 * HOURS,
            decision_period: 14 * DAYS,
            confirm_period: 6 * HOURS,
            min_enactment_period: 20 * MINUTES,
            min_approval: Curve::make_reciprocal(4, 7, percent(80), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 7, percent(20), percent(5), percent(50)),
        },
    },
    // 5 — general_admin: routine administrative calls on the runtime's pallets
    // (XCM admin, collator set updates, and similar).
    pallet_referenda::Track {
        id: 5,
        info: pallet_referenda::TrackInfo {
            name: s("general_admin"),
            max_deciding: 3,
            decision_deposit: 1_000 * UNITS,
            prepare_period: 2 * HOURS,
            decision_period: 7 * DAYS,
            confirm_period: 4 * HOURS,
            min_enactment_period: 20 * MINUTES,
            min_approval: Curve::make_reciprocal(3, 5, percent(75), percent(50), percent(100)),
            min_support: Curve::make_reciprocal(1, 5, percent(15), percent(5), percent(50)),
        },
    },
];

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
    type Id = u16;
    type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;

    fn tracks() -> impl Iterator<Item = Cow<'static, pallet_referenda::Track<Self::Id, Balance, BlockNumber>>>
    {
        TRACKS_DATA.iter().map(Cow::Borrowed)
    }

    fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
        if let Ok(system_origin) = frame_system::RawOrigin::try_from(id.clone()) {
            match system_origin {
                frame_system::RawOrigin::Root => Ok(0),
                _ => Err(()),
            }
        } else if let Ok(custom_origin) = origins::Origin::try_from(id.clone()) {
            match custom_origin {
                origins::Origin::WhitelistedCaller => Ok(1),
                origins::Origin::ReferendumCanceller => Ok(2),
                origins::Origin::ReferendumKiller => Ok(3),
                origins::Origin::TreasurySpend => Ok(4),
                origins::Origin::GeneralAdmin => Ok(5),
            }
        } else {
            Err(())
        }
    }
}
