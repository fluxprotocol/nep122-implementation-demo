use near_crypto::{InMemorySigner, KeyType, Signer};
use near_runtime_standalone::{
    init_runtime_and_signer, 
    RuntimeStandalone
};

use near_primitives::{
    account::{AccessKey},
    errors::{RuntimeError, TxExecutionError},
    hash::CryptoHash,
    transaction::{ExecutionOutcome, ExecutionStatus, Transaction},
    types::{ AccountId, Balance },
};

use near_sdk::{
    json_types::{
        U128,
    }
};

use serde_json::json;

pub struct ExternalUser {
    account_id: AccountId,
    signer: InMemorySigner,
}

const GAS_STANDARD: u64 = 10000000000000000;
const NEAR_DEPOSIT: u128 = 30000000000000000000000;


type TxResult = Result<ExecutionOutcome, ExecutionOutcome>;

lazy_static::lazy_static! {
    static ref CONTRACT_BYTES: &'static [u8] = include_bytes!("../../../res/near_contract_template.wasm").as_ref();
    static ref CONTRACT_CONSUMER_BYTES: &'static [u8] = include_bytes!("../../../nep122_consumer.wasm").as_ref();
}

fn contract_dev() -> String {
	"contract_dev".to_string()
}

pub fn consumer_contract() -> String {
    "contract_consumer_dev".to_string()
}

pub fn ntoy(near_amount: Balance) -> Balance {
    near_amount * 10u128.pow(24)
}

fn outcome_into_result(outcome: ExecutionOutcome) -> TxResult {
    match outcome.status {
        ExecutionStatus::SuccessValue(_) => Ok(outcome),
        ExecutionStatus::Failure(_) => Err(outcome),
        ExecutionStatus::SuccessReceiptId(_) => panic!("Unresolved ExecutionOutcome run runtime.resolve(tx) to resolve the final outcome of tx"),
        ExecutionStatus::Unknown => unreachable!()
    }
}

impl ExternalUser {
    pub fn new(account_id: AccountId, signer: InMemorySigner) -> Self {
        Self { account_id, signer }
    }

    pub fn get_account_id(&self) -> AccountId {
        self.account_id.to_string()
    }

    fn new_tx(&self, runtime: &RuntimeStandalone, receiver_id: AccountId) -> Transaction {
        let nonce = runtime
        .view_access_key(&self.account_id, &self.signer.public_key())
        .unwrap()
        .nonce
        + 1;
        Transaction::new(
            self.account_id.clone(),
            self.signer.public_key(),
            receiver_id,
            nonce,
            CryptoHash::default(),
        )
    }

    pub fn deploy_contract(&self, runtime: &mut RuntimeStandalone) -> TxResult {
        let args = json!({
            "owner_id": self.get_account_id(),
            "total_supply": U128(1_000_000),
        }).to_string().as_bytes().to_vec();

        let tx = self
            .new_tx(runtime, contract_dev())
            .create_account()
            .transfer(99994508400000000000000000)
            .deploy_contract(CONTRACT_BYTES.to_vec())
            .function_call("init".into(), args, 300000000000000, 0)
            .sign(&self.signer);

        let res = runtime.resolve_tx(tx).unwrap();
        runtime.process_all().unwrap();
        outcome_into_result(res)
    }

    pub fn deploy_consumer_contract(&self, runtime: &mut RuntimeStandalone) -> TxResult {
        let args = json!({
            "token_contract_id": contract_dev(),
        }).to_string().as_bytes().to_vec();

        let tx = self
            .new_tx(runtime, consumer_contract())
            .create_account()
            .transfer(99994508400000000000000000)
            .deploy_contract(CONTRACT_CONSUMER_BYTES.to_vec())
            .function_call("init".into(), args, 300000000000000, 0)
            .sign(&self.signer);

        let res = runtime.resolve_tx(tx).unwrap();
        runtime.process_all().unwrap();
        outcome_into_result(res)
    }

    pub fn create_external(
        &self,
        runtime: &mut RuntimeStandalone,
        new_account_id: AccountId,
        amount: Balance,
    ) -> Result<ExternalUser, ExecutionOutcome> {
        let new_signer = InMemorySigner::from_seed(&new_account_id, KeyType::ED25519, &new_account_id);
        let tx = self
            .new_tx(runtime, new_account_id.clone())
            .create_account()
            .add_key(new_signer.public_key(), AccessKey::full_access())
            .transfer(amount)
            .sign(&self.signer);

        let res = runtime.resolve_tx(tx);

        // TODO: this temporary hack, must be rewritten
        if let Err(err) = res.clone() {
            if let RuntimeError::InvalidTxError(tx_err) = err {
                let mut out = ExecutionOutcome::default();
                out.status = ExecutionStatus::Failure(TxExecutionError::InvalidTxError(tx_err));
                Err(out)
            } else {
                unreachable!();
            }
        } else {
            outcome_into_result(res.unwrap())?;
            runtime.process_all().unwrap();
            Ok(ExternalUser {
                account_id: new_account_id,
                signer: new_signer,
            })
        }
    }
    
    /** Actual contract methods */
    pub fn transfer_with_safe(&self, runtime: &mut RuntimeStandalone, receiver_id: AccountId, amount: U128, payload: String) -> TxResult {
        let args = json!({
            "receiver_id": receiver_id,
            "amount": amount,
            "payload": payload,
        })
            .to_string()
            .as_bytes()
            .to_vec();

        let tx = self
            .new_tx(runtime, contract_dev())
            .function_call("transfer_with_safe".into(), args, GAS_STANDARD, 0)
            .sign(&self.signer);

        let res = runtime.resolve_tx(tx).expect("resolving tx failed");
        runtime.process_all().expect("processing tx failed");

        outcome_into_result(res)
    }
}

pub fn init_near_contract() -> (RuntimeStandalone, ExternalUser) {
    let (mut runtime, signer) = init_runtime_and_signer(&"contract-dev".into());
    let root = ExternalUser::new("contract-dev".into(), signer);

    root.deploy_contract(&mut runtime).unwrap();
    root.deploy_consumer_contract(&mut runtime).unwrap();
    
    (runtime, root)
}