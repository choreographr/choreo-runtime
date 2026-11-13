//! Custom governance origins for the Choreographr/CHOR OpenGov stack.
//!
//! These are the origins that the referenda tracks dispatch as once a referendum
//! passes. `Root` additionally exists via `frame_system::RawOrigin::Root`, so
//! sudo (while it is still installed) and the `root` track can both act as root.

pub use custom_origins::*;

#[polkadot_sdk::frame_support::pallet]
pub mod custom_origins {
    use crate::{Balance, UNITS};
    use polkadot_sdk::frame_support::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config {}

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[derive(
        PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug,
    )]
    #[pallet::origin]
    pub enum Origin {
        /// Origin able to dispatch a whitelisted call.
        WhitelistedCaller,
        /// Origin able to cancel referenda.
        ReferendumCanceller,
        /// Origin able to kill referenda.
        ReferendumKiller,
        /// Origin able to execute `treasury.spend`.
        TreasurySpend,
        /// Origin for general administrative actions across pallets.
        GeneralAdmin,
    }

    macro_rules! decl_unit_ensures {
        ( $name:ident: $success_type:ty = $success:expr ) => {
            pub struct $name;
            impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for $name
            where
                for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
            {
                type Success = $success_type;
                fn try_origin(o: O) -> Result<Self::Success, O> {
                    match o.caller().try_into() {
                        Ok(Origin::$name) => return Ok($success),
                        _ => (),
                    }

                    Err(o)
                }
                #[cfg(feature = "runtime-benchmarks")]
                fn try_successful_origin() -> Result<O, ()> {
                    Ok(O::from(Origin::$name))
                }
            }
        };
        ( $name:ident ) => { decl_unit_ensures! { $name : () = () } };
        ( $name:ident: $success_type:ty = $success:expr, $( $rest:tt )* ) => {
            decl_unit_ensures! { $name: $success_type = $success }
            decl_unit_ensures! { $( $rest )* }
        };
        ( $name:ident, $( $rest:tt )* ) => {
            decl_unit_ensures! { $name }
            decl_unit_ensures! { $( $rest )* }
        };
        () => {}
    }
    decl_unit_ensures!(
        ReferendumCanceller,
        ReferendumKiller,
        WhitelistedCaller,
        GeneralAdmin,
        TreasurySpend,
    );

    macro_rules! decl_ensure {
        (
            $vis:vis type $name:ident: EnsureOrigin<Success = $success_type:ty> {
                $( $item:ident = $success:expr, )*
            }
        ) => {
            $vis struct $name;
            impl<O: OriginTrait + From<Origin> + TryFrom<Origin>> EnsureOrigin<O> for $name
            where
                for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
            {
                type Success = $success_type;
                fn try_origin(o: O) -> Result<Self::Success, O> {
                    match o.caller().try_into() {
                        $(
                            Ok(Origin::$item) => return Ok($success),
                        )*
                        _ => (),
                    }

                    Err(o)
                }
                #[cfg(feature = "runtime-benchmarks")]
                fn try_successful_origin() -> Result<O, ()> {
                    // By convention the more privileged origins go later, so for greatest chance
                    // of success, we want the last one.
                    let _result: Result<O, ()> = Err(());
                    $(
                        let _result: Result<O, ()> = Ok(O::from(Origin::$item));
                    )*
                    _result
                }
            }
        }
    }

    /// The maximum amount a `treasury_spend` referendum may spend in one call.
    pub const TREASURY_SPEND_LIMIT: Balance = 100_000 * UNITS;

    decl_ensure! {
        pub type Spender: EnsureOrigin<Success = Balance> {
            TreasurySpend = TREASURY_SPEND_LIMIT,
        }
    }
}
