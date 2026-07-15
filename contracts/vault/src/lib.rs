//! # StellarAid Vault Contract
//!
//! A milestone-based fund disbursement vault for the StellarAid platform
//! on the Stellar blockchain via Soroban.
//!
//! ## Overview
//!
//! This contract manages charitable donations with full transparency:
//! - Donors deposit tokens into the vault.
//! - An admin proposes milestones with specific recipients and amounts.
//! - A multisig quorum of signers must approve each milestone.
//! - Once approved, funds are released directly to the milestone recipient.
//! - Donors may reclaim unallocated funds via refund.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Status of a milestone in its lifecycle.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Milestone has been proposed by the admin but not yet approved.
    Proposed,
    /// Milestone has received the required number of multisig approvals.
    Active,
    /// Funds have been released to the recipient.
    Completed,
    /// Milestone was rejected (reserved for future governance).
    Rejected,
}

/// A single milestone representing a discrete unit of work to be funded.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Unique auto-incremented identifier.
    pub id: u32,
    /// Human-readable title (e.g., "Build Water Well").
    pub title: String,
    /// Detailed description of the milestone deliverable.
    pub description: String,
    /// Token amount to be released upon completion.
    pub amount: i128,
    /// Current lifecycle status.
    pub status: MilestoneStatus,
    /// Address that will receive funds when the milestone is released.
    pub recipient: Address,
}

/// Storage keys used across all contract state.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The admin address that can propose milestones.
    Admin,
    /// Address of the Stellar Asset Contract (SAC) token used by this vault.
    TokenAddress,
    /// Cumulative amount deposited into the vault.
    TotalDeposited,
    /// Cumulative amount released to milestone recipients.
    TotalReleased,
    /// Cumulative amount refunded to donors.
    TotalRefunded,
    /// Auto-increment counter for milestone IDs.
    MilestoneCount,
    /// List of authorized signer addresses for multisig approval.
    Signers,
    /// Number of signer approvals required to activate a milestone.
    RequiredSigs,
    /// A specific milestone, keyed by its ID.
    Milestone(u32),
    /// Per-donor deposit balance tracker.
    Deposit(Address),
    /// Tracks whether a specific signer has approved a specific milestone.
    Approval(u32, Address),
}

// ---------------------------------------------------------------------------
// Contract definition
// ---------------------------------------------------------------------------

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the vault with an admin, token address, signer set, and
    /// the required number of signer approvals.
    ///
    /// # Panics
    /// - If `required_sigs` exceeds the number of signers.
    /// - If `required_sigs` is zero.
    /// - If the signer list is empty.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        signers: Vec<Address>,
        required_sigs: u32,
    ) {
        // Prevent re-initialization.
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("vault already initialized");
        }

        // Validate signer configuration.
        if signers.is_empty() {
            panic!("signers list must not be empty");
        }
        if required_sigs == 0 {
            panic!("required_sigs must be at least 1");
        }
        if required_sigs > signers.len() {
            panic!("required_sigs cannot exceed the number of signers");
        }

        // Persist configuration in instance storage (global, cheap reads).
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenAddress, &token);
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage()
            .instance()
            .set(&DataKey::RequiredSigs, &required_sigs);
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposited, &0_i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalReleased, &0_i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalRefunded, &0_i128);
        env.storage()
            .instance()
            .set(&DataKey::MilestoneCount, &0_u32);
    }

    // -----------------------------------------------------------------------
    // Deposits
    // -----------------------------------------------------------------------

    /// Deposit `amount` of the configured token into the vault.
    ///
    /// The donor must authorize the transaction. Tokens are transferred from
    /// the donor to this contract's address.
    ///
    /// # Panics
    /// - If `amount` is not positive.
    /// - If arithmetic overflow occurs.
    pub fn deposit(env: Env, donor: Address, amount: i128) {
        // Require the donor to authorize this call.
        donor.require_auth();

        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        // Transfer tokens from donor to the vault contract.
        let token_address: Address =
            env.storage().instance().get(&DataKey::TokenAddress).unwrap();
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);

        // Update per-donor deposit balance (persistent storage).
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Deposit(donor.clone()))
            .unwrap_or(0);
        let new_balance = current_balance
            .checked_add(amount)
            .expect("deposit overflow");
        env.storage()
            .persistent()
            .set(&DataKey::Deposit(donor), &new_balance);

        // Update global total deposited.
        let total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap();
        let new_total = total.checked_add(amount).expect("total deposit overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposited, &new_total);
    }

    // -----------------------------------------------------------------------
    // Milestone management
    // -----------------------------------------------------------------------

    /// Add a new milestone. Only the admin may call this.
    ///
    /// Milestones are created in `Proposed` status and require multisig
    /// approval before funds can be released.
    ///
    /// # Returns
    /// The auto-incremented milestone ID.
    ///
    /// # Panics
    /// - If the caller is not the admin.
    /// - If the milestone amount is not positive.
    pub fn add_milestone(
        env: Env,
        caller: Address,
        title: String,
        description: String,
        amount: i128,
        recipient: Address,
    ) -> u32 {
        caller.require_auth();

        // Verify caller is admin.
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != admin {
            panic!("only admin can add milestones");
        }

        if amount <= 0 {
            panic!("milestone amount must be positive");
        }

        // Auto-increment milestone ID.
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MilestoneCount)
            .unwrap();
        let milestone_id = count;

        let milestone = Milestone {
            id: milestone_id,
            title,
            description,
            amount,
            status: MilestoneStatus::Proposed,
            recipient,
        };

        // Store milestone in persistent storage (keyed by ID).
        env.storage()
            .persistent()
            .set(&DataKey::Milestone(milestone_id), &milestone);

        // Increment counter.
        env.storage()
            .instance()
            .set(&DataKey::MilestoneCount, &(milestone_id + 1));

        milestone_id
    }

    /// Approve a milestone as one of the authorized signers.
    ///
    /// Once the number of unique approvals meets `required_sigs`, the
    /// milestone status transitions from `Proposed` to `Active`.
    ///
    /// # Panics
    /// - If the signer is not in the authorized signers list.
    /// - If the signer has already approved this milestone.
    /// - If the milestone is not in `Proposed` status.
    pub fn approve_milestone(env: Env, signer: Address, milestone_id: u32) {
        signer.require_auth();

        // Verify the signer is authorized.
        let signers: Vec<Address> =
            env.storage().instance().get(&DataKey::Signers).unwrap();
        let mut is_valid_signer = false;
        for s in signers.iter() {
            if s == signer {
                is_valid_signer = true;
                break;
            }
        }
        if !is_valid_signer {
            panic!("caller is not an authorized signer");
        }

        // Prevent duplicate approvals.
        let approval_key = DataKey::Approval(milestone_id, signer.clone());
        if env.storage().persistent().has(&approval_key) {
            panic!("signer has already approved this milestone");
        }

        // Ensure milestone exists and is in Proposed status.
        let mut milestone: Milestone = env
            .storage()
            .persistent()
            .get(&DataKey::Milestone(milestone_id))
            .expect("milestone not found");
        if milestone.status != MilestoneStatus::Proposed {
            panic!("milestone is not in Proposed status");
        }

        // Record approval.
        env.storage().persistent().set(&approval_key, &true);

        // Count total approvals for this milestone.
        let all_signers: Vec<Address> =
            env.storage().instance().get(&DataKey::Signers).unwrap();
        let required: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RequiredSigs)
            .unwrap();

        let mut approval_count: u32 = 0;
        for s in all_signers.iter() {
            let key = DataKey::Approval(milestone_id, s);
            if env.storage().persistent().has(&key) {
                approval_count += 1;
            }
        }

        // If quorum is reached, transition to Active.
        if approval_count >= required {
            milestone.status = MilestoneStatus::Active;
            env.storage()
                .persistent()
                .set(&DataKey::Milestone(milestone_id), &milestone);
        }
    }

    // -----------------------------------------------------------------------
    // Fund release
    // -----------------------------------------------------------------------

    /// Release funds for an approved (Active) milestone to its recipient.
    ///
    /// Transfers the milestone amount from the vault to the designated
    /// recipient address and marks the milestone as Completed.
    ///
    /// # Panics
    /// - If the milestone is not in `Active` status.
    /// - If arithmetic overflow occurs.
    pub fn release_milestone(env: Env, milestone_id: u32) {
        let mut milestone: Milestone = env
            .storage()
            .persistent()
            .get(&DataKey::Milestone(milestone_id))
            .expect("milestone not found");

        if milestone.status != MilestoneStatus::Active {
            panic!("milestone must be Active to release funds");
        }

        // Transfer tokens from vault to recipient.
        let token_address: Address =
            env.storage().instance().get(&DataKey::TokenAddress).unwrap();
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(
            &env.current_contract_address(),
            &milestone.recipient,
            &milestone.amount,
        );

        // Mark as completed.
        milestone.status = MilestoneStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Milestone(milestone_id), &milestone);

        // Update total released.
        let total_released: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReleased)
            .unwrap();
        let new_total = total_released
            .checked_add(milestone.amount)
            .expect("total released overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalReleased, &new_total);
    }

    // -----------------------------------------------------------------------
    // Refunds
    // -----------------------------------------------------------------------

    /// Refund the donor's entire deposited balance back to them.
    ///
    /// This transfers the donor's tracked deposit amount from the vault
    /// contract back to the donor and clears their deposit record.
    ///
    /// # Panics
    /// - If the donor has no balance to refund.
    /// - If arithmetic overflow occurs.
    pub fn refund(env: Env, donor: Address) {
        donor.require_auth();

        let deposit_key = DataKey::Deposit(donor.clone());
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&deposit_key)
            .unwrap_or(0);

        if balance <= 0 {
            panic!("no funds available for refund");
        }

        // Transfer tokens back to donor.
        let token_address: Address =
            env.storage().instance().get(&DataKey::TokenAddress).unwrap();
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &donor, &balance);

        // Clear donor deposit record.
        env.storage().persistent().remove(&deposit_key);

        // Update total refunded.
        let total_refunded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefunded)
            .unwrap();
        let new_total = total_refunded
            .checked_add(balance)
            .expect("total refunded overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalRefunded, &new_total);
    }

    // -----------------------------------------------------------------------
    // Read-only views
    // -----------------------------------------------------------------------

    /// Returns the full milestone struct for the given ID.
    ///
    /// # Panics
    /// - If the milestone does not exist.
    pub fn get_milestone_status(env: Env, milestone_id: u32) -> Milestone {
        env.storage()
            .persistent()
            .get(&DataKey::Milestone(milestone_id))
            .expect("milestone not found")
    }

    /// Returns a tuple of (total_deposited, total_released, total_refunded).
    pub fn get_vault_info(env: Env) -> (i128, i128, i128) {
        let deposited: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0);
        let released: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReleased)
            .unwrap_or(0);
        let refunded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefunded)
            .unwrap_or(0);
        (deposited, released, refunded)
    }

    /// Returns the current deposit balance for a given donor address.
    pub fn get_donor_balance(env: Env, donor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Deposit(donor))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test;

// TODO: Review performance constraints here (Ref: 758712d9 - 1784119191)

// TODO: Review performance constraints here (Ref: e39ca0d4 - 1784119207)
