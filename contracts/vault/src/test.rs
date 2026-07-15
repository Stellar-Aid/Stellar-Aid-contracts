#![cfg(test)]

//! Unit tests for the StellarAid Vault contract.
//!
//! These tests exercise the full public interface defined in `lib.rs`
//! against an in-memory Soroban test environment. A Stellar Asset Contract
//! (SAC) is registered to stand in for the vault's token, and its
//! `StellarAssetClient` is used to mint balances to donors.

use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, String, Vec,
};

use crate::{Milestone, MilestoneStatus, VaultContract, VaultContractClient};

// ---------------------------------------------------------------------------
// Test harness helpers
// ---------------------------------------------------------------------------

/// Bundles together everything a test needs: the environment, the deployed
/// vault client, the admin, the signer set, and the token clients.
struct VaultTest<'a> {
    env: Env,
    admin: Address,
    signers: Vec<Address>,
    vault: VaultContractClient<'a>,
    token: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
}

impl<'a> VaultTest<'a> {
    /// Set up a fresh environment with a registered vault and SAC token.
    ///
    /// The vault is NOT initialized here so that tests can control the
    /// initialization parameters (and test the un-initialized path).
    fn setup(num_signers: u32) -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);

        // Register a Stellar Asset Contract to act as the vault token.
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_address = sac.address();
        let token = token::Client::new(&env, &token_address);
        let token_admin = token::StellarAssetClient::new(&env, &token_address);

        // Build a signer set.
        let mut signers: Vec<Address> = Vec::new(&env);
        for _ in 0..num_signers {
            signers.push_back(Address::generate(&env));
        }

        // Deploy the vault contract (soroban-sdk 21 API).
        let contract_id = env.register(VaultContract, ());
        let vault = VaultContractClient::new(&env, &contract_id);

        VaultTest {
            env,
            admin,
            signers,
            vault,
            token,
            token_admin,
        }
    }

    /// The vault contract address (for balance assertions).
    fn vault_address(&self) -> Address {
        self.vault.address.clone()
    }

    /// Mint `amount` of the token to `to`.
    fn mint(&self, to: &Address, amount: i128) {
        self.token_admin.mint(to, &amount);
    }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_configuration() {
    let t = VaultTest::setup(3);
    t.vault.initialize(
        &t.admin,
        &t.token.address,
        &t.signers,
        &2u32,
    );

    // Fresh vault: all accounting counters are zero.
    let (deposited, released, refunded) = t.vault.get_vault_info();
    assert_eq!(deposited, 0);
    assert_eq!(released, 0);
    assert_eq!(refunded, 0);
}

#[test]
#[should_panic(expected = "vault already initialized")]
fn test_reinitialize_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);
    // Second call must panic.
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);
}

#[test]
#[should_panic(expected = "signers list must not be empty")]
fn test_initialize_empty_signers_panics() {
    let t = VaultTest::setup(0);
    let empty: Vec<Address> = Vec::new(&t.env);
    t.vault
        .initialize(&t.admin, &t.token.address, &empty, &1u32);
}

#[test]
#[should_panic(expected = "required_sigs must be at least 1")]
fn test_initialize_zero_required_sigs_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &0u32);
}

#[test]
#[should_panic(expected = "required_sigs cannot exceed the number of signers")]
fn test_initialize_required_exceeds_signers_panics() {
    let t = VaultTest::setup(2);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &5u32);
}

// ---------------------------------------------------------------------------
// Deposits
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_happy_path() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);

    t.vault.deposit(&donor, &400i128);

    // Tokens moved from donor into the vault.
    assert_eq!(t.token.balance(&donor), 600);
    assert_eq!(t.token.balance(&t.vault_address()), 400);

    // Per-donor and global accounting reflect the deposit.
    assert_eq!(t.vault.get_donor_balance(&donor), 400);
    let (deposited, _, _) = t.vault.get_vault_info();
    assert_eq!(deposited, 400);
}

#[test]
fn test_multiple_deposits_accumulate() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);

    t.vault.deposit(&donor, &100i128);
    t.vault.deposit(&donor, &250i128);

    assert_eq!(t.vault.get_donor_balance(&donor), 350);
    let (deposited, _, _) = t.vault.get_vault_info();
    assert_eq!(deposited, 350);
    assert_eq!(t.token.balance(&t.vault_address()), 350);
}

#[test]
#[should_panic(expected = "deposit amount must be positive")]
fn test_deposit_zero_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &0i128);
}

#[test]
#[should_panic(expected = "deposit amount must be positive")]
fn test_deposit_negative_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &-5i128);
}

// ---------------------------------------------------------------------------
// Milestone creation
// ---------------------------------------------------------------------------

#[test]
fn test_add_milestone_by_admin() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Build Water Well"),
        &String::from_str(&t.env, "Dig and equip a well for the village"),
        &500i128,
        &recipient,
    );

    // First milestone is ID 0.
    assert_eq!(id, 0);

    let m: Milestone = t.vault.get_milestone_status(&id);
    assert_eq!(m.id, 0);
    assert_eq!(m.amount, 500);
    assert_eq!(m.recipient, recipient);
    assert_eq!(m.status, MilestoneStatus::Proposed);
    assert_eq!(m.title, String::from_str(&t.env, "Build Water Well"));
}

#[test]
fn test_add_milestone_ids_auto_increment() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let first = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "M0"),
        &String::from_str(&t.env, "first"),
        &100i128,
        &recipient,
    );
    let second = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "M1"),
        &String::from_str(&t.env, "second"),
        &200i128,
        &recipient,
    );

    assert_eq!(first, 0);
    assert_eq!(second, 1);
}

#[test]
#[should_panic(expected = "only admin can add milestones")]
fn test_add_milestone_by_non_admin_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let impostor = Address::generate(&t.env);
    let recipient = Address::generate(&t.env);
    t.vault.add_milestone(
        &impostor,
        &String::from_str(&t.env, "rogue"),
        &String::from_str(&t.env, "unauthorized"),
        &100i128,
        &recipient,
    );
}

#[test]
#[should_panic(expected = "milestone amount must be positive")]
fn test_add_milestone_non_positive_amount_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "bad"),
        &String::from_str(&t.env, "zero amount"),
        &0i128,
        &recipient,
    );
}

// ---------------------------------------------------------------------------
// Multisig approval flow (2-of-3)
// ---------------------------------------------------------------------------

#[test]
fn test_full_multisig_approval_activates_milestone() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );

    let s0 = t.signers.get(0).unwrap();
    let s1 = t.signers.get(1).unwrap();

    // First approval: still Proposed (quorum of 2 not yet met).
    t.vault.approve_milestone(&s0, &id);
    assert_eq!(
        t.vault.get_milestone_status(&id).status,
        MilestoneStatus::Proposed
    );

    // Second approval reaches the 2-of-3 quorum -> Active.
    t.vault.approve_milestone(&s1, &id);
    assert_eq!(
        t.vault.get_milestone_status(&id).status,
        MilestoneStatus::Active
    );
}

#[test]
#[should_panic(expected = "caller is not an authorized signer")]
fn test_approve_by_non_signer_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );

    let outsider = Address::generate(&t.env);
    t.vault.approve_milestone(&outsider, &id);
}

#[test]
#[should_panic(expected = "signer has already approved this milestone")]
fn test_duplicate_approval_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );

    let s0 = t.signers.get(0).unwrap();
    t.vault.approve_milestone(&s0, &id);
    // Same signer approves again -> panic.
    t.vault.approve_milestone(&s0, &id);
}

#[test]
#[should_panic(expected = "milestone is not in Proposed status")]
fn test_approve_after_active_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );

    let s0 = t.signers.get(0).unwrap();
    let s1 = t.signers.get(1).unwrap();
    let s2 = t.signers.get(2).unwrap();

    // Reach quorum with two approvals -> Active.
    t.vault.approve_milestone(&s0, &id);
    t.vault.approve_milestone(&s1, &id);
    // A third signer approving an already-Active milestone must panic.
    t.vault.approve_milestone(&s2, &id);
}

// ---------------------------------------------------------------------------
// Release flow
// ---------------------------------------------------------------------------

#[test]
fn test_release_milestone_transfers_and_completes() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    // Fund the vault via a donor deposit.
    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &500i128);

    // Propose and approve a milestone.
    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );
    t.vault.approve_milestone(&t.signers.get(0).unwrap(), &id);
    t.vault.approve_milestone(&t.signers.get(1).unwrap(), &id);

    // Release funds.
    t.vault.release_milestone(&id);

    // Recipient received tokens; vault balance reduced accordingly.
    assert_eq!(t.token.balance(&recipient), 300);
    assert_eq!(t.token.balance(&t.vault_address()), 200);

    // Milestone marked Completed.
    assert_eq!(
        t.vault.get_milestone_status(&id).status,
        MilestoneStatus::Completed
    );

    // Global accounting reflects the release.
    let (deposited, released, refunded) = t.vault.get_vault_info();
    assert_eq!(deposited, 500);
    assert_eq!(released, 300);
    assert_eq!(refunded, 0);
}

#[test]
#[should_panic(expected = "milestone must be Active to release funds")]
fn test_release_when_not_active_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &500i128);

    // Milestone is only Proposed (no approvals) -> release must panic.
    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );
    t.vault.release_milestone(&id);
}

#[test]
#[should_panic(expected = "milestone must be Active to release funds")]
fn test_double_release_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &500i128);

    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Milestone"),
        &String::from_str(&t.env, "desc"),
        &300i128,
        &recipient,
    );
    t.vault.approve_milestone(&t.signers.get(0).unwrap(), &id);
    t.vault.approve_milestone(&t.signers.get(1).unwrap(), &id);

    // First release completes the milestone.
    t.vault.release_milestone(&id);
    // Second release must panic because it is now Completed, not Active.
    t.vault.release_milestone(&id);
}

// ---------------------------------------------------------------------------
// Refunds
// ---------------------------------------------------------------------------

#[test]
fn test_refund_returns_donor_funds() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &400i128);

    // Sanity: funds are held by the vault.
    assert_eq!(t.token.balance(&donor), 600);
    assert_eq!(t.token.balance(&t.vault_address()), 400);

    t.vault.refund(&donor);

    // Donor made whole; vault emptied; deposit record cleared.
    assert_eq!(t.token.balance(&donor), 1_000);
    assert_eq!(t.token.balance(&t.vault_address()), 0);
    assert_eq!(t.vault.get_donor_balance(&donor), 0);

    let (deposited, released, refunded) = t.vault.get_vault_info();
    assert_eq!(deposited, 400);
    assert_eq!(released, 0);
    assert_eq!(refunded, 400);
}

#[test]
#[should_panic(expected = "no funds available for refund")]
fn test_refund_with_no_balance_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    // Donor never deposited -> nothing to refund.
    let donor = Address::generate(&t.env);
    t.vault.refund(&donor);
}

#[test]
#[should_panic(expected = "no funds available for refund")]
fn test_double_refund_panics() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    let donor = Address::generate(&t.env);
    t.mint(&donor, 1_000);
    t.vault.deposit(&donor, &400i128);

    t.vault.refund(&donor);
    // Deposit record was cleared, so a second refund must panic.
    t.vault.refund(&donor);
}

// ---------------------------------------------------------------------------
// End-to-end accounting
// ---------------------------------------------------------------------------

#[test]
fn test_end_to_end_accounting() {
    let t = VaultTest::setup(3);
    t.vault
        .initialize(&t.admin, &t.token.address, &t.signers, &2u32);

    // Two donors deposit.
    let donor_a = Address::generate(&t.env);
    let donor_b = Address::generate(&t.env);
    t.mint(&donor_a, 1_000);
    t.mint(&donor_b, 1_000);
    t.vault.deposit(&donor_a, &600i128);
    t.vault.deposit(&donor_b, &400i128);

    // One milestone released, then donor B refunded.
    let recipient = Address::generate(&t.env);
    let id = t.vault.add_milestone(
        &t.admin,
        &String::from_str(&t.env, "Deliverable"),
        &String::from_str(&t.env, "desc"),
        &500i128,
        &recipient,
    );
    t.vault.approve_milestone(&t.signers.get(0).unwrap(), &id);
    t.vault.approve_milestone(&t.signers.get(2).unwrap(), &id);
    t.vault.release_milestone(&id);
    t.vault.refund(&donor_b);

    let (deposited, released, refunded) = t.vault.get_vault_info();
    assert_eq!(deposited, 1_000);
    assert_eq!(released, 500);
    assert_eq!(refunded, 400);

    // Remaining vault balance = deposits - released - refunded.
    assert_eq!(t.token.balance(&t.vault_address()), 100);
    assert_eq!(t.token.balance(&recipient), 500);
    assert_eq!(t.token.balance(&donor_b), 1_000);
}

// TODO: Review performance constraints here (Ref: 5ac4f417 - 1784119189)

// TODO: Review performance constraints here (Ref: 49f688c5 - 1784119194)

// TODO: Review performance constraints here (Ref: 5563c7e2 - 1784119199)

// TODO: Review performance constraints here (Ref: 9b2f93e9 - 1784119221)

// TODO: Review performance constraints here (Ref: d1c7a2d2 - 1784119228)

// TODO: Review performance constraints here (Ref: b74a0200 - 1784119232)

// TODO: Review performance constraints here (Ref: bf1148ab - 1784119246)

// TODO: Review performance constraints here (Ref: 2ded2b67 - 1784119256)

// TODO: Review performance constraints here (Ref: 722ee0a1 - 1784119260)

// TODO: Review performance constraints here (Ref: 0f74288f - 1784119272)

// TODO: Review performance constraints here (Ref: 5d4423fc - 1784119287)

// TODO: Review performance constraints here (Ref: b1bd72c2 - 1784119293)

// TODO: Review performance constraints here (Ref: 4a8d3523 - 1784119321)

// TODO: Review performance constraints here (Ref: 29b69230 - 1784119329)
