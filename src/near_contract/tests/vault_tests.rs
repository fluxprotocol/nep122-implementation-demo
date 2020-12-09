use super::*;
use near_sdk::{
    json_types::{
        U128,
    }
};
use near_primitives::{
    transaction::{ ExecutionStatus }
};

use contract_user::{ consumer_contract };

#[test]
fn test_transfering_with_vault() {
    let (mut runtime, root, _accounts) = init_runtime_env();
    let tx_result = root.transfer_with_safe(&mut runtime, consumer_contract(), U128(100), "".to_string()).unwrap();

    assert_eq!(tx_result.status, ExecutionStatus::SuccessValue(b"\"10\"".to_vec()));
}
