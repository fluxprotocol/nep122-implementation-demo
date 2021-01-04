use super::*;

/// Load in contract bytes
near_sdk_sim::lazy_static! {
    static ref TOKEN_WASM_BYTES: &'static [u8] = include_bytes!("../../res/vault_token.wasm").as_ref();
}

fn init(
    initial_balance: u128,
) -> (UserAccount, ContractAccount<VaultTokenContract>, UserAccount) {
    let master_account = init_simulator(None);
    // uses default values for deposit and gas
    let contract_user = deploy!(
        // Contract Proxy
        contract: VaultTokenContract,
        // Contract account id
        contract_id: "contract",
        // Bytes of contract
        bytes: &TOKEN_WASM_BYTES,
        // User deploying the contract,
        signer_account: master_account,
        // init method
        init_method: init(master_account.account_id(), initial_balance.into())
    );
    let alice = master_account.create_user("alice".to_string(), to_yocto("100"));
    (master_account, contract_user, alice)
}

#[test]
fn test_sim_transfer() {
    let transfer_amount = to_yocto("100");
    let initial_balance = to_yocto("100000");
    let (master_account, contract, alice) = init(initial_balance);
    
    let registration_res = call!(
        master_account,
        contract.register_account(alice.account_id.clone()),
        deposit = STORAGE_AMOUNT
    );

    assert!(registration_res.is_ok());

    let transfer_res = call!(
        master_account,
        contract.transfer(alice.account_id.clone(), transfer_amount.into()),
        deposit = 0
    );
    assert!(transfer_res.is_ok());

    let value = view!(contract.get_balance(master_account.account_id()));
    let value: U128 = value.unwrap_json();
    assert_eq!(initial_balance - transfer_amount, value.0);
}