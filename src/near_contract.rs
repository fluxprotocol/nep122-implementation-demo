use near_sdk::{
    near_bindgen,
    json_types::{
        U128,
    },
    serde::{
        Serialize,
        Deserialize,
    },
    ext_contract,
    AccountId,
    Gas,
    Balance,
    collections::{
		LookupMap,
	},
    Promise,
    env,
    borsh::{
        self,
        BorshDeserialize,
        BorshSerialize,
    },
};

use serde_json::json;

const GAS_BASE_COMPUTE: Gas = 5_000_000_000_000;
const GAS_FOR_CALLBACK: Gas = GAS_BASE_COMPUTE;
const GAS_FOR_PROMISE: Gas = 5_000_000_000_000;
const GAS_FOR_DATA_DEPENDENCY: Gas = 10_000_000_000_000;
const GAS_FOR_REMAINING_COMPUTE: Gas = 2 * GAS_FOR_PROMISE + GAS_FOR_DATA_DEPENDENCY + GAS_BASE_COMPUTE;

/// Safe identifier.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultId(pub u64);

impl VaultId {
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Vault {
    pub sender_id: AccountId,
    pub receiver_id: AccountId,
    pub balance: Balance,
}

#[ext_contract(ext_token_receiver)]
trait ExtTokenReceiver {
    fn on_receive_with_vault(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        vault_id: VaultId,
        payload: String,
    ) -> Promise;
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_vault(&mut self, vault_id: VaultId, sender_id: AccountId) -> U128;
}

#[derive(BorshDeserialize, BorshSerialize)]
struct Token {
    pub accounts: LookupMap<AccountId, Balance>,
    pub total_supply: Balance,
}

impl Token {
    pub fn new(owner_id: AccountId, total_supply: u128) -> Self {
        let mut accounts = LookupMap::new(b"balance".to_vec());
        accounts.insert(&owner_id, &total_supply);

        Self {
            total_supply: total_supply.clone(),
            accounts,
        }
    }

    pub fn deposit(&mut self, receiver_id: AccountId, amount: u128) {
        assert!(amount > 0, "Cannot deposit 0 or lower");

        let receiver_balance = self.accounts.get(&receiver_id).unwrap_or(0);
        self.accounts.insert(&receiver_id, &(receiver_balance + amount));
    }

    pub fn withdraw(&mut self, sender_id: AccountId, amount: u128) {
        let sender_balance = self.accounts.get(&sender_id).unwrap_or(0);

        assert!(amount > 0, "Cannot withdraw 0 or lower");
        assert!(sender_balance >= amount, "Not enough balance");

        self.accounts.insert(&sender_id, &(sender_balance - amount));
    }
}


// NEP 122
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
struct VaultFungibleToken {
    pub token: Token,
    pub vaults: LookupMap<VaultId, Vault>,
    pub next_vault_id: VaultId,
}

impl Default for VaultFungibleToken {
    fn default() -> Self {
        panic!("Contract should be initialized before usage")
    }
}

#[near_bindgen]
impl VaultFungibleToken {
    #[init]
    pub fn init(owner_id: AccountId, total_supply: U128) -> Self {
        Self {
            token: Token::new(owner_id, total_supply.into()),
            vaults: LookupMap::new(b"vaults".to_vec()),
            next_vault_id: VaultId(0),
        }
    }

    #[payable]
    pub fn transfer_unsafe(&mut self, receiver_id: AccountId, amount: U128) {
        self.token.withdraw(env::predecessor_account_id(), amount.into());
        self.token.deposit(receiver_id, amount.into());
    }

    #[payable]
    pub fn transfer_with_safe(&mut self, receiver_id: AccountId, amount: U128, payload: String) -> Promise {
        let gas_to_receiver = env::prepaid_gas().saturating_sub(GAS_FOR_REMAINING_COMPUTE + GAS_FOR_CALLBACK);
        let vault_id = self.next_vault_id;
        let sender_id = env::predecessor_account_id();

        self.token.withdraw(sender_id.to_string(), amount.into());
        self.next_vault_id = vault_id.next();

        let vault = Vault {
            balance: amount.into(),
            sender_id: sender_id.to_string(),
            receiver_id: receiver_id.to_string(),
        };

        self.vaults.insert(&vault_id, &vault);

        ext_token_receiver::on_receive_with_vault(
            sender_id.to_string(), 
            amount,
            vault_id, 
            payload,
            &receiver_id,
            0,
            gas_to_receiver,
        )
        .then(ext_self::resolve_vault(
            vault_id,
            sender_id,
            &env::current_account_id(),
            0,
            GAS_FOR_CALLBACK,
        ))
    }

    pub fn resolve_vault(&mut self, vault_id: VaultId, sender_id: AccountId) -> U128 {
        assert_eq!(env::current_account_id(), env::predecessor_account_id(), "Private method can only be called by contract");

        let vault = self.vaults.remove(&vault_id).expect("Vault does not exist");

        env::log(json!({
            "type": U128(vault.balance),
        }).to_string().as_bytes());

        if vault.balance > 0 {
            self.token.deposit(sender_id, vault.balance);
        }

        vault.balance.into()
    }

    pub fn withdraw_from_vault(&mut self, vault_id: VaultId, receiver_id: AccountId, amount: U128) {
        env::log(json!({
            "type": "Withdrawing money"
        }).to_string().as_bytes());

        let mut vault = self.vaults.get(&vault_id).expect("Vault does not exist");
        assert!(env::predecessor_account_id() == vault.receiver_id, "Access of vault denied");
        
        let amount_to_withdraw: u128 = amount.into();
        assert!(amount_to_withdraw < vault.balance, "Not enough balance inside vault");

        vault.balance -= amount_to_withdraw;
        self.vaults.insert(&vault_id, &vault);
        self.token.deposit(receiver_id, amount.into());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    mod contract_user;
    use contract_user::{ init_near_contract, ntoy, ExternalUser };
    use near_sdk::{ AccountId, VMContext };
    use near_runtime_standalone::{ RuntimeStandalone };

    fn alice() -> AccountId {
		"alice.near".to_string()
	}

	fn bob() -> AccountId {
		"bob.near".to_string()
    }
    
    fn get_context(
		predecessor_account_id: AccountId, 
		block_timestamp: u64
	) -> VMContext {

		VMContext {
			current_account_id: alice(),
            signer_account_id: bob(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
			block_index: 0,
			epoch_height: 0,
            account_balance: 0,
			is_view: false,
            storage_usage: 0,
			block_timestamp,
			account_locked_balance: 0,
            attached_deposit: 0,
            prepaid_gas: 10_u64.pow(12),
            random_seed: vec![0, 1, 2],
            output_data_receivers: vec![],
		}
    }
    
    fn init_runtime_env() -> (RuntimeStandalone, ExternalUser, Vec<ExternalUser>) {
        let (mut runtime, root) = init_near_contract();
        let mut accounts: Vec<ExternalUser> = vec![];

        for acc_no in 0..2 {
			let acc = if let Ok(acc) =
				root.create_external(&mut runtime, format!("account_{}", acc_no), ntoy(100))
			{
				acc
			} else {
				break;
            };
            
			accounts.push(acc);
        }
        
        (runtime, root, accounts)
    }

    mod vault_tests;
}