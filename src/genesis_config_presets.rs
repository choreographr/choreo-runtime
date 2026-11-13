//! Genesis presets supplied to `chain-spec-builder` / `polkadot-omni-node`.
//!
//! `development` is a throwaway single-authority chain for local work. Real
//! deployments (own relay chain, Paseo, Polkadot) do not use a preset: they
//! supply a JSON patch carrying the actual collator keys, the ACU-derived
//! balances, the para id and the sudo/admin account.

use super::*;
use crate::{
    BalancesConfig, CollatorSelectionConfig, ParachainInfoConfig, RuntimeGenesisConfig,
    SessionConfig, SessionKeys, SudoConfig,
};
use alloc::vec;
use cumulus_primitives_core::ParaId;
use serde_json::Value;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_keyring::Sr25519Keyring;

fn dev_authority() -> AuraId {
    Sr25519Keyring::Alice.public().into()
}

pub fn development_config_genesis() -> Value {
    let alice = Sr25519Keyring::Alice.to_account_id();
    let bob = Sr25519Keyring::Bob.to_account_id();

    frame_support::build_struct_json_patch!(RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: vec![(alice.clone(), 10 * UNITS), (bob, 10 * UNITS)]
        },
        aura: pallet_aura::GenesisConfig {
            authorities: vec![dev_authority()],
        },
        session: SessionConfig {
            keys: vec![(
                alice.clone(),
                alice.clone(),
                SessionKeys {
                    aura: dev_authority(),
                }
            )],
            non_authority_keys: vec![],
        },
        collator_selection: CollatorSelectionConfig {
            invulnerables: vec![alice.clone()],
            candidacy_bond: 0,
            desired_candidates: 0,
        },
        parachain_info: ParachainInfoConfig {
            parachain_id: ParaId::new(1000),
        },
        sudo: SudoConfig {
            key: Some(alice),
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
