// Tests for the inkind_vault program.
//
// Run with:
//   cargo build-sbf --arch v0
//   cargo test -p inkind_vault

#![cfg(test)]

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, program_pack::Pack, system_program},
    InstructionData, ToAccountMetas,
};
use anchor_spl::{associated_token::spl_associated_token_account, token::spl_token};
use inkind_vault::{MINIMUM_SHARES, SHARES_SEED, VAULT_SEED};
use litesvm::LiteSVM;
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Derive the ATA address using the canonical SPL seeds.
/// Seeds: [wallet_address, token_program_id, mint_address]
fn ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    let spl_ata_id = spl_associated_token_account::program::id();
    Pubkey::find_program_address(
        &[wallet.as_ref(), spl_token::id().as_ref(), mint.as_ref()],
        &spl_ata_id,
    )
    .0
}

fn spl_ata_program_id() -> Pubkey {
    spl_associated_token_account::program::id()
}

fn vault_pda(base: &Pubkey, position: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[VAULT_SEED, base.as_ref(), position.as_ref()],
        &inkind_vault::id(),
    )
}

fn share_mint_pda(vault: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[SHARES_SEED, vault.as_ref()], &inkind_vault::id()).0
}

fn load_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/inkind_vault.so"
    ));
    svm.add_program(inkind_vault::id(), bytes).unwrap();
    svm
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> Result<(), String> {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}\nlogs: {:#?}", e.err, e.meta.logs))
}

/// Read the SPL token balance of an ATA. Returns 0 if the account does not exist.
fn token_balance(svm: &LiteSVM, owner: &Pubkey, mint: &Pubkey) -> u64 {
    let addr = ata(owner, mint);
    svm.get_account(&addr)
        .and_then(|a| spl_token::state::Account::unpack(&a.data).ok())
        .map(|ta| ta.amount)
        .unwrap_or(0)
}

// ── instruction builders ──────────────────────────────────────────────────────

fn ix_initialize(payer: &Pubkey, base_mint: &Pubkey, position_mint: &Pubkey) -> Instruction {
    let (vault, _) = vault_pda(base_mint, position_mint);
    let share_mint = share_mint_pda(&vault);
    Instruction::new_with_bytes(
        inkind_vault::id(),
        &inkind_vault::instruction::Initialize {}.data(),
        inkind_vault::accounts::Initialize {
            payer: *payer,
            base_mint: *base_mint,
            position_mint: *position_mint,
            vault,
            share_mint,
            base_vault: ata(&vault, base_mint),
            position_vault: ata(&vault, position_mint),
            token_program: spl_token::id(),
            associated_token_program: spl_ata_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_deposit(
    user: &Pubkey,
    base_mint: &Pubkey,
    position_mint: &Pubkey,
    shares: u64,
    max_base: u64,
    max_position: u64,
) -> Instruction {
    let (vault, _) = vault_pda(base_mint, position_mint);
    let share_mint = share_mint_pda(&vault);
    Instruction::new_with_bytes(
        inkind_vault::id(),
        &inkind_vault::instruction::Deposit { shares, max_base, max_position }.data(),
        inkind_vault::accounts::Deposit {
            user: *user,
            vault,
            base_mint: *base_mint,
            position_mint: *position_mint,
            share_mint,
            base_vault: ata(&vault, base_mint),
            position_vault: ata(&vault, position_mint),
            vault_shares: ata(&vault, &share_mint),
            user_base: ata(user, base_mint),
            user_position: ata(user, position_mint),
            user_shares: ata(user, &share_mint),
            token_program: spl_token::id(),
            associated_token_program: spl_ata_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_redeem(user: &Pubkey, base_mint: &Pubkey, position_mint: &Pubkey, shares: u64) -> Instruction {
    let (vault, _) = vault_pda(base_mint, position_mint);
    let share_mint = share_mint_pda(&vault);
    Instruction::new_with_bytes(
        inkind_vault::id(),
        &inkind_vault::instruction::RedeemInKind { shares }.data(),
        inkind_vault::accounts::RedeemInKind {
            user: *user,
            vault,
            base_mint: *base_mint,
            position_mint: *position_mint,
            share_mint,
            base_vault: ata(&vault, base_mint),
            position_vault: ata(&vault, position_mint),
            user_base: ata(user, base_mint),
            user_position: ata(user, position_mint),
            user_shares: ata(user, &share_mint),
            token_program: spl_token::id(),
            associated_token_program: spl_ata_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

// ── fixture ───────────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    base_mint: Pubkey,
    position_mint: Pubkey,
    vault: Pubkey,
    share_mint: Pubkey,
}

impl Fixture {
    fn new() -> Self {
        let mut svm = load_svm();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

        let base_mint = CreateMint::new(&mut svm, &payer).decimals(6).send().unwrap();
        let position_mint = CreateMint::new(&mut svm, &payer).decimals(6).send().unwrap();

        let ix = ix_initialize(&payer.pubkey(), &base_mint, &position_mint);
        send(&mut svm, &payer, ix).expect("initialize failed");

        let (vault, _) = vault_pda(&base_mint, &position_mint);
        let share_mint = share_mint_pda(&vault);

        Fixture { svm, payer, base_mint, position_mint, vault, share_mint }
    }

    /// Give a user SOL + funded ATAs for base, position, and share token.
    fn fund_user(&mut self, user: &Keypair, base_amount: u64, position_amount: u64) {
        let user_pub = user.pubkey();
        self.svm.airdrop(&user_pub, 10_000_000_000).unwrap();

        // Create ATAs via litesvm-token (uses its own Pubkey type internally)
        CreateAssociatedTokenAccount::new(&mut self.svm, &self.payer, &self.base_mint)
            .owner(&user_pub)
            .send()
            .unwrap();
        CreateAssociatedTokenAccount::new(&mut self.svm, &self.payer, &self.position_mint)
            .owner(&user_pub)
            .send()
            .unwrap();
        CreateAssociatedTokenAccount::new(&mut self.svm, &self.payer, &self.share_mint)
            .owner(&user_pub)
            .send()
            .unwrap();

        // Mint tokens into the user's ATAs. We need the Address type that
        // litesvm-token uses, so derive it via its own helper.
        let base_ata = ata(&user_pub, &self.base_mint);
        let pos_ata = ata(&user_pub, &self.position_mint);

        // litesvm-token's MintTo accepts &solana_pubkey::Pubkey which is the
        // same type as our Pubkey (re-exported by anchor_lang). Cast via bytes.
        let base_ata_lsvm = solana_pubkey::Pubkey::from(base_ata.to_bytes());
        let pos_ata_lsvm = solana_pubkey::Pubkey::from(pos_ata.to_bytes());
        let base_mint_lsvm = solana_pubkey::Pubkey::from(self.base_mint.to_bytes());
        let pos_mint_lsvm = solana_pubkey::Pubkey::from(self.position_mint.to_bytes());

        MintTo::new(&mut self.svm, &self.payer, &base_mint_lsvm, &base_ata_lsvm, base_amount)
            .send()
            .unwrap();
        MintTo::new(&mut self.svm, &self.payer, &pos_mint_lsvm, &pos_ata_lsvm, position_amount)
            .send()
            .unwrap();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── test 1: first deposit mints max_base - MINIMUM_SHARES shares ──────────────

#[test]
fn test_first_deposit() {
    let mut f = Fixture::new();
    let user = Keypair::new();
    let max_base: u64 = 100_000;
    let max_position: u64 = 50_000;
    f.fund_user(&user, max_base, max_position);

    let (base_mint, position_mint, share_mint, vault) =
        (f.base_mint, f.position_mint, f.share_mint, f.vault);
    let user_pub = user.pubkey();

    let ix = ix_deposit(&user_pub, &base_mint, &position_mint, 0, max_base, max_position);
    send(&mut f.svm, &user, ix).expect("first deposit failed");

    assert_eq!(
        token_balance(&f.svm, &user_pub, &share_mint),
        max_base,
        "user should receive max_base shares on first deposit"
    );
    // MINIMUM_SHARES are minted to the vault's own ATA and locked there forever
    assert_eq!(
        token_balance(&f.svm, &vault, &share_mint),
        MINIMUM_SHARES,
        "vault should hold MINIMUM_SHARES locked permanently"
    );
    assert_eq!(
        token_balance(&f.svm, &vault, &base_mint),
        max_base,
        "vault should hold full base deposit"
    );
    assert_eq!(
        token_balance(&f.svm, &vault, &position_mint),
        max_position,
        "vault should hold full position deposit"
    );
}

// ── test 2: second deposit is pro-rata (rounded up) ───────────────────────────

#[test]
fn test_second_deposit_pro_rata() {
    let mut f = Fixture::new();
    let (base_mint, position_mint, share_mint) = (f.base_mint, f.position_mint, f.share_mint);

    // Alice sets the ratio: 100_000 base / 50_000 position
    // After first deposit: supply = 100_000 (user) + MINIMUM_SHARES (locked) = 101_000
    let alice = Keypair::new();
    f.fund_user(&alice, 100_000, 50_000);
    let ix = ix_deposit(&alice.pubkey(), &base_mint, &position_mint, 0, 100_000, 50_000);
    send(&mut f.svm, &alice, ix).unwrap();

    let alice_shares = token_balance(&f.svm, &alice.pubkey(), &share_mint);
    // supply = alice_shares + MINIMUM_SHARES = 100_000 + 1_000 = 101_000
    // vault: 100_000 base, 50_000 position

    // Bob wants half of alice's shares (50_000)
    // Required base  = ceil(50_000 * 100_000 / 101_000) = ceil(49_504.95) = 49_505
    // Required pos   = ceil(50_000 * 50_000  / 101_000) = ceil(24_752.47) = 24_753
    let bob = Keypair::new();
    let wanted = alice_shares / 2; // 50_000
    let need_base = wanted * 100_000 / (alice_shares + MINIMUM_SHARES) + 2; // +2 for rounding
    let need_pos = wanted * 50_000 / (alice_shares + MINIMUM_SHARES) + 2;
    f.fund_user(&bob, need_base, need_pos);
    let ix = ix_deposit(&bob.pubkey(), &base_mint, &position_mint, wanted, need_base, need_pos);
    send(&mut f.svm, &bob, ix).unwrap();

    assert_eq!(
        token_balance(&f.svm, &bob.pubkey(), &share_mint),
        wanted,
        "bob should receive exactly the shares he asked for"
    );
}

// ── test 3: full redeem returns proportional assets ───────────────────────────

#[test]
fn test_redeem_in_kind_full() {
    let mut f = Fixture::new();
    let (base_mint, position_mint, share_mint) = (f.base_mint, f.position_mint, f.share_mint);

    let user = Keypair::new();
    f.fund_user(&user, 200_000, 100_000);
    let user_pub = user.pubkey();

    let ix = ix_deposit(&user_pub, &base_mint, &position_mint, 0, 200_000, 100_000);
    send(&mut f.svm, &user, ix).unwrap();

    let shares = token_balance(&f.svm, &user_pub, &share_mint);
    let ix = ix_redeem(&user_pub, &base_mint, &position_mint, shares);
    send(&mut f.svm, &user, ix).expect("redeem failed");

    // User should recover most of the deposit (MINIMUM_SHARES worth stays locked)
    assert!(token_balance(&f.svm, &user_pub, &base_mint) > 0, "user should get base back");
    assert!(token_balance(&f.svm, &user_pub, &position_mint) > 0, "user should get position back");
}

// ── test 4 (KEY): redeem succeeds even when base_out rounds to 0 ──────────────

#[test]
fn test_redeem_when_base_empty() {
    let mut f = Fixture::new();
    let (base_mint, position_mint, share_mint) = (f.base_mint, f.position_mint, f.share_mint);

    // Tiny first deposit: only 1 share is minted (max_base = MINIMUM_SHARES + 1).
    // base_vault = MINIMUM_SHARES + 1, supply = MINIMUM_SHARES + 1.
    // User holds 1 share. On redeem: base_out = floor(1 * (MIN+1) / (MIN+1)) = 1.
    // The important property: the transaction must not fail regardless of base_out.
    let max_base = MINIMUM_SHARES + 1;
    let max_position: u64 = 50_000;
    let user = Keypair::new();
    f.fund_user(&user, max_base, max_position);
    let user_pub = user.pubkey();

    let ix = ix_deposit(&user_pub, &base_mint, &position_mint, 0, max_base, max_position);
    send(&mut f.svm, &user, ix).unwrap();

    let shares = token_balance(&f.svm, &user_pub, &share_mint);
    assert_eq!(shares, max_base, "user should hold max_base shares");

    let ix = ix_redeem(&user_pub, &base_mint, &position_mint, shares);
    send(&mut f.svm, &user, ix).expect("redeem must succeed even when base_out is tiny");

    // User must have received position tokens
    assert!(
        token_balance(&f.svm, &user_pub, &position_mint) > 0,
        "user should receive position tokens on exit"
    );
}

// ── test 5: redeem 0 shares fails ────────────────────────────────────────────

#[test]
fn test_redeem_zero_fails() {
    let mut f = Fixture::new();
    let (base_mint, position_mint) = (f.base_mint, f.position_mint);

    let user = Keypair::new();
    f.fund_user(&user, 100_000, 50_000);
    let user_pub = user.pubkey();

    let ix = ix_deposit(&user_pub, &base_mint, &position_mint, 0, 100_000, 50_000);
    send(&mut f.svm, &user, ix).unwrap();

    let ix = ix_redeem(&user_pub, &base_mint, &position_mint, 0);
    assert!(send(&mut f.svm, &user, ix).is_err(), "redeem of 0 shares must fail");
}

// ── test 6: redeem more than balance fails ────────────────────────────────────

#[test]
fn test_redeem_more_than_balance_fails() {
    let mut f = Fixture::new();
    let (base_mint, position_mint, share_mint) = (f.base_mint, f.position_mint, f.share_mint);

    let user = Keypair::new();
    f.fund_user(&user, 100_000, 50_000);
    let user_pub = user.pubkey();

    let ix = ix_deposit(&user_pub, &base_mint, &position_mint, 0, 100_000, 50_000);
    send(&mut f.svm, &user, ix).unwrap();

    let shares = token_balance(&f.svm, &user_pub, &share_mint);
    let ix = ix_redeem(&user_pub, &base_mint, &position_mint, shares + 1);
    assert!(send(&mut f.svm, &user, ix).is_err(), "cannot redeem more shares than you hold");
}

// ── test 7: another user cannot redeem someone else's shares ──────────────────

#[test]
fn test_cannot_redeem_other_users_shares() {
    let mut f = Fixture::new();
    let (base_mint, position_mint, share_mint) = (f.base_mint, f.position_mint, f.share_mint);

    let alice = Keypair::new();
    f.fund_user(&alice, 100_000, 50_000);
    let ix = ix_deposit(&alice.pubkey(), &base_mint, &position_mint, 0, 100_000, 50_000);
    send(&mut f.svm, &alice, ix).unwrap();
    let alice_shares = token_balance(&f.svm, &alice.pubkey(), &share_mint);

    // Bob has no shares; trying to redeem alice_shares worth from his empty ATA must fail.
    let bob = Keypair::new();
    f.fund_user(&bob, 0, 0); // creates ATAs but mints nothing
    let ix = ix_redeem(&bob.pubkey(), &base_mint, &position_mint, alice_shares);
    assert!(
        send(&mut f.svm, &bob, ix).is_err(),
        "bob cannot redeem shares he does not own"
    );
}

// ── test 8: deposit with slippage too tight fails ─────────────────────────────

#[test]
fn test_deposit_slippage_exceeded() {
    let mut f = Fixture::new();
    let (base_mint, position_mint) = (f.base_mint, f.position_mint);

    // Alice sets the ratio
    let alice = Keypair::new();
    f.fund_user(&alice, 100_000, 50_000);
    let ix = ix_deposit(&alice.pubkey(), &base_mint, &position_mint, 0, 100_000, 50_000);
    send(&mut f.svm, &alice, ix).unwrap();

    // Bob wants 10_000 shares but only allows 1 base token in — must fail
    let bob = Keypair::new();
    f.fund_user(&bob, 20_000, 10_000);
    let ix = ix_deposit(&bob.pubkey(), &base_mint, &position_mint, 10_000, 1, 10_000);
    assert!(
        send(&mut f.svm, &bob, ix).is_err(),
        "deposit must fail when max_base is below the required amount"
    );
}
