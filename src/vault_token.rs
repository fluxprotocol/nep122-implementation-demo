use near_sdk::{
    near_bindgen,
    StorageUsage,
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
    serde_json::json
};

const GAS_BASE_COMPUTE: Gas = 5_000_000_000_000;
const GAS_FOR_CALLBACK: Gas = GAS_BASE_COMPUTE;
const GAS_FOR_PROMISE: Gas = 5_000_000_000_000;
const GAS_FOR_DATA_DEPENDENCY: Gas = 10_000_000_000_000;
const GAS_FOR_REMAINING_COMPUTE: Gas = 2 * GAS_FOR_PROMISE + GAS_FOR_DATA_DEPENDENCY + GAS_BASE_COMPUTE;
const STORAGE_PRICE_PER_BYTE: Balance = 100_000_000_000_000_000_000;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, PartialEq)]
pub struct ShortAccountHash(pub [u8; 20]);

impl From<&AccountId> for ShortAccountHash {
    fn from(account_id: &AccountId) -> Self {
        let mut buf = [0u8; 20];
        buf.copy_from_slice(&env::sha256(account_id.as_bytes())[..20]);
        Self(buf)
    }
}

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
    pub receiver_id: ShortAccountHash,
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
    fn resolve_vault(&mut self, vault_id: VaultId, sender_id: ShortAccountHash) -> U128;
}

#[derive(BorshDeserialize, BorshSerialize)]
struct Token {
    pub accounts: LookupMap<ShortAccountHash, Balance>,
    pub total_supply: Balance,
}

impl Token {
    pub fn new(owner_id: AccountId, total_supply: u128) -> Self {
        let mut accounts = LookupMap::new(b"balance".to_vec());
        accounts.insert(&ShortAccountHash::from(&owner_id), &total_supply);

        Self {
            total_supply: total_supply.clone(),
            accounts,
        }
    }

    pub fn register_account(&mut self, account_id: AccountId) {
        let current_storage = env::storage_usage();
        let short_account_id = ShortAccountHash::from(&account_id);
        let balance = self.accounts.get(&short_account_id);
        if balance.is_some() { panic!("ERR_IS_REGISTERED") }
        self.accounts.insert(&short_account_id, &0);
        self.refund_storage(current_storage);
    }

    pub fn unregister_account(&mut self) {
        let current_storage = env::storage_usage();
        let short_account_id = ShortAccountHash::from(&env::predecessor_account_id());
        let balance = self.accounts.get(&short_account_id).expect("ERR_NOT_REGISTERED");
        assert!(balance == 0, "ERR_INVALID_BALANCE");
        self.accounts.insert(&short_account_id, &0);
        self.refund_storage(current_storage);
    }

    pub fn deposit(&mut self, receiver_id: &ShortAccountHash, amount: u128) {
        assert!(amount > 0, "Cannot deposit 0 or lower");

        let receiver_balance = self.accounts.get(receiver_id).expect("ERR_UNREGISTERED_ACCOUNT");
        self.accounts.insert(receiver_id, &(receiver_balance + amount));
    }

    pub fn withdraw(&mut self, sender_id: &ShortAccountHash, amount: u128) {
        let sender_balance = self.accounts.get(sender_id).expect("ERR_UNREGISTERED_ACCOUNT");

        assert!(amount > 0, "Cannot withdraw 0 or lower");
        assert!(sender_balance >= amount, "Not enough balance");

        self.accounts.insert(sender_id, &(sender_balance - amount));
    }
}

impl Token {
    fn refund_storage(&self, initial_storage: StorageUsage) {
        let current_storage = env::storage_usage();
        let attached_deposit = env::attached_deposit();
        let refund_amount = if current_storage > initial_storage {
            let required_deposit =
                Balance::from(current_storage - initial_storage) * STORAGE_PRICE_PER_BYTE;
            assert!(
                required_deposit <= attached_deposit,
                "The required attached deposit is {}, but the given attached deposit is is {}",
                required_deposit,
                attached_deposit,
            );
            attached_deposit - required_deposit
        } else {
            attached_deposit
                + Balance::from(initial_storage - current_storage) * STORAGE_PRICE_PER_BYTE
        };
        if refund_amount > 0 {
            env::log(format!("Refunding {} tokens for storage", refund_amount).as_bytes());
            Promise::new(env::predecessor_account_id()).transfer(refund_amount);
        }
    }
}


// NEP 122
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
struct VaultToken {
    pub token: Token,
    pub vaults: LookupMap<VaultId, Vault>,
    pub next_vault_id: VaultId,
}

impl Default for VaultToken {
    fn default() -> Self {
        panic!("Contract should be initialized before usage")
    }
}

#[near_bindgen]
impl VaultToken {
    #[init]
    pub fn init(owner_id: AccountId, total_supply: U128) -> Self {
        Self {
            token: Token::new(owner_id, total_supply.into()),
            vaults: LookupMap::new(b"vaults".to_vec()),
            next_vault_id: VaultId(0),
        }
    }

    #[payable]
    pub fn register_account(&mut self, account_id: AccountId) {
        self.token.register_account(account_id);
    }
    
    #[payable]
    pub fn unregister_account(&mut self) {
        self.token.unregister_account();    
    }

    pub fn get_balance(&self, account_id: AccountId) -> U128 {
        let to = ShortAccountHash::from(&account_id);
        self.token.accounts.get(&to).unwrap_or(0).into()
    }

    pub fn transfer(&mut self, receiver_id: AccountId, amount: U128) {
        let from = ShortAccountHash::from(&env::predecessor_account_id());
        let to = ShortAccountHash::from(&receiver_id);
        self.token.withdraw(&from, amount.into());
        self.token.deposit(&to, amount.into());
    }

    #[payable]
    pub fn transfer_with_vault(&mut self, receiver_id: AccountId, amount: U128, payload: String) -> Promise {
        let gas_to_receiver = env::prepaid_gas().saturating_sub(GAS_FOR_REMAINING_COMPUTE + GAS_FOR_CALLBACK);
        let vault_id = self.next_vault_id;
        let from = ShortAccountHash::from(&env::predecessor_account_id());
        let to = ShortAccountHash::from(&receiver_id);

        self.token.withdraw(&from, amount.into());
        self.next_vault_id = vault_id.next();

        let vault = Vault {
            balance: amount.into(),
            receiver_id: to,
        };

        self.vaults.insert(&vault_id, &vault);

        ext_token_receiver::on_receive_with_vault(
            env::predecessor_account_id(), 
            amount,
            vault_id, 
            payload,
            &receiver_id,
            env::attached_deposit(),
            gas_to_receiver,
        )
        .then(ext_self::resolve_vault(
            vault_id,
            from,
            &env::current_account_id(),
            0,
            GAS_FOR_CALLBACK,
        ))
    }

    pub fn resolve_vault(&mut self, vault_id: VaultId, sender_id: ShortAccountHash) -> U128 {
        assert_eq!(env::current_account_id(), env::predecessor_account_id(), "Private method can only be called by contract");

        let vault = self.vaults.remove(&vault_id).expect("Vault does not exist");

        env::log(json!({
            "type": U128(vault.balance),
        }).to_string().as_bytes());

        if vault.balance > 0 {
            self.token.deposit(&sender_id, vault.balance);
        }

        vault.balance.into()
    }

    pub fn withdraw_from_vault(&mut self, vault_id: VaultId, receiver_id: AccountId, amount: U128) {
        let mut vault = self.vaults.get(&vault_id).expect("Vault does not exist");
        let vault_receiver_id = ShortAccountHash::from(&env::predecessor_account_id());
        
        assert!(&vault_receiver_id == &vault.receiver_id, "The vault is not owned by the predecessor");
        let amount_to_withdraw: u128 = amount.into();
        assert!(amount_to_withdraw <= vault.balance, "Not enough balance inside vault");

        vault.balance -= amount_to_withdraw;
        self.vaults.insert(&vault_id, &vault);
        self.token.deposit(&vault_receiver_id, amount.into());
    }
}
