use near_sdk_sim::{
    call, deploy, init_simulator, near_crypto::Signer, to_yocto, view, ContractAccount,
    UserAccount, STORAGE_AMOUNT,
};
use std::str::FromStr;

/// Bring contract crate into namespace
use crate::vault_token;
/// Import the generated proxy contract
use vault_token::VaultTokenContract;
use near_sdk::json_types::U128;
use near_sdk_sim::account::AccessKey;

mod general;