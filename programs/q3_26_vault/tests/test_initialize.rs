use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    q3_26_vault::{STATE, VAULT_SEED},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const DEPOSIT_LAMPORTS: u64 = 500_000_000;
const WITHDRAW_LAMPORTS: u64 = 100_000_000;
// Upper bound for the fee of one transaction (the real fee is 5,000 lamports)
const MAX_FEE: u64 = 20_000;

// Sends one instruction and returns Ok(()) or the error text with the program logs
fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> Result<(), String> {
    // A fresh blockhash lets us send the same instruction twice without "AlreadyProcessed"
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|err| format!("{:?}\nlogs: {:#?}", err.err, err.meta.logs))
}

// Everything a test needs: the SVM, one user and that user's PDAs
struct Fixture {
    svm: LiteSVM,
    user: Keypair,
    vault_state: Pubkey,
    vault: Pubkey,
}

fn pdas(user: &Pubkey) -> (Pubkey, Pubkey) {
    let program_id = q3_26_vault::id();
    let (vault_state, _) = Pubkey::find_program_address(&[STATE, user.as_ref()], &program_id);
    let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED, user.as_ref()], &program_id);
    (vault_state, vault)
}

fn setup() -> Fixture {
    let user = Keypair::new();
    let (vault_state, vault) = pdas(&user.pubkey());

    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/q3_26_vault.so"
    ));
    svm.add_program(q3_26_vault::id(), bytes).unwrap();
    svm.airdrop(&user.pubkey(), 2_000_000_000).unwrap();

    Fixture {
        svm,
        user,
        vault_state,
        vault,
    }
}

fn ix_initialize(f: &Fixture) -> Instruction {
    Instruction::new_with_bytes(
        q3_26_vault::id(),
        &q3_26_vault::instruction::Initialize {}.data(),
        q3_26_vault::accounts::Initialize {
            user: f.user.pubkey(),
            vault_state: f.vault_state,
            vault: f.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_deposit(f: &Fixture, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        q3_26_vault::id(),
        &q3_26_vault::instruction::Deposit { amount }.data(),
        q3_26_vault::accounts::Deposit {
            user: f.user.pubkey(),
            vault_state: f.vault_state,
            vault: f.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_withdraw(f: &Fixture, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        q3_26_vault::id(),
        &q3_26_vault::instruction::Withdraw { amount }.data(),
        q3_26_vault::accounts::Withdraw {
            user: f.user.pubkey(),
            vault_state: f.vault_state,
            vault: f.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_close(f: &Fixture) -> Instruction {
    Instruction::new_with_bytes(
        q3_26_vault::id(),
        &q3_26_vault::instruction::Close {}.data(),
        q3_26_vault::accounts::Close {
            user: f.user.pubkey(),
            vault_state: f.vault_state,
            vault: f.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

// A fixture that already ran initialize and deposit
fn setup_with_deposit() -> Fixture {
    let mut f = setup();
    let ix = ix_initialize(&f);
    send(&mut f.svm, &f.user, ix).unwrap();
    let ix = ix_deposit(&f, DEPOSIT_LAMPORTS);
    send(&mut f.svm, &f.user, ix).unwrap();
    f
}

fn is_closed(svm: &LiteSVM, address: &Pubkey) -> bool {
    svm.get_account(address)
        .map(|account| account.lamports == 0)
        .unwrap_or(true)
}

#[test]
fn test_initialize_deposit_withdraw_close() {
    let mut f = setup();
    let rent_exempt = f.svm.minimum_balance_for_rent_exemption(0);

    // initialize: creates the state account and funds the vault with its rent
    let ix = ix_initialize(&f);
    send(&mut f.svm, &f.user, ix).unwrap();
    assert!(
        f.svm.get_account(&f.vault_state).is_some(),
        "vault_state should exist after initialize"
    );
    assert_eq!(f.svm.get_balance(&f.vault).unwrap(), rent_exempt);

    // deposit
    let ix = ix_deposit(&f, DEPOSIT_LAMPORTS);
    send(&mut f.svm, &f.user, ix).unwrap();
    assert_eq!(
        f.svm.get_balance(&f.vault).unwrap(),
        rent_exempt + DEPOSIT_LAMPORTS,
        "vault should increase by the deposit"
    );

    // withdraw
    let user_before = f.svm.get_balance(&f.user.pubkey()).unwrap();
    let ix = ix_withdraw(&f, WITHDRAW_LAMPORTS);
    send(&mut f.svm, &f.user, ix).unwrap();
    assert_eq!(
        f.svm.get_balance(&f.vault).unwrap(),
        rent_exempt + DEPOSIT_LAMPORTS - WITHDRAW_LAMPORTS,
        "vault should decrease by the withdraw"
    );
    // the user receives the withdrawn lamports, minus a small transaction fee
    let user_after = f.svm.get_balance(&f.user.pubkey()).unwrap();
    assert!(
        user_after <= user_before + WITHDRAW_LAMPORTS
            && user_after >= user_before + WITHDRAW_LAMPORTS - MAX_FEE,
        "user should receive the withdrawn lamports minus the fee"
    );

    // close
    let ix = ix_close(&f);
    send(&mut f.svm, &f.user, ix).unwrap();
    assert!(
        is_closed(&f.svm, &f.vault_state),
        "vault_state should be closed"
    );
    assert!(is_closed(&f.svm, &f.vault), "vault should be empty");
}

#[test]
fn test_deposit_zero_fails() {
    let mut f = setup();
    let ix = ix_initialize(&f);
    send(&mut f.svm, &f.user, ix).unwrap();

    let ix = ix_deposit(&f, 0);
    let err = send(&mut f.svm, &f.user, ix).unwrap_err();
    assert!(err.contains("InvalidAmount"), "unexpected error: {err}");
}

#[test]
fn test_withdraw_zero_fails() {
    let mut f = setup_with_deposit();
    let ix = ix_withdraw(&f, 0);
    let err = send(&mut f.svm, &f.user, ix).unwrap_err();
    assert!(err.contains("InvalidAmount"), "unexpected error: {err}");
}

#[test]
fn test_withdraw_more_than_balance_fails() {
    let mut f = setup_with_deposit();
    let balance = f.svm.get_balance(&f.vault).unwrap();

    let ix = ix_withdraw(&f, balance + 1);
    let err = send(&mut f.svm, &f.user, ix).unwrap_err();
    assert!(err.contains("InsufficientFunds"), "unexpected error: {err}");
    assert_eq!(f.svm.get_balance(&f.vault).unwrap(), balance);
}

#[test]
fn test_withdraw_cannot_break_rent_exemption() {
    let mut f = setup_with_deposit();
    let rent_exempt = f.svm.minimum_balance_for_rent_exemption(0);

    // One lamport more than the deposit would leave the vault below the rent minimum
    let ix = ix_withdraw(&f, DEPOSIT_LAMPORTS + 1);
    let err = send(&mut f.svm, &f.user, ix).unwrap_err();
    assert!(err.contains("InsufficientFunds"), "unexpected error: {err}");

    // Withdrawing exactly the deposit is fine: the rent stays in the vault
    let ix = ix_withdraw(&f, DEPOSIT_LAMPORTS);
    send(&mut f.svm, &f.user, ix).unwrap();
    assert_eq!(f.svm.get_balance(&f.vault).unwrap(), rent_exempt);
}

#[test]
fn test_other_user_cannot_withdraw() {
    let mut f = setup_with_deposit();
    let balance = f.svm.get_balance(&f.vault).unwrap();

    // The attacker signs, but points to the victim's vault and state accounts
    let attacker = Keypair::new();
    f.svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();
    let ix = Instruction::new_with_bytes(
        q3_26_vault::id(),
        &q3_26_vault::instruction::Withdraw {
            amount: WITHDRAW_LAMPORTS,
        }
        .data(),
        q3_26_vault::accounts::Withdraw {
            user: attacker.pubkey(),
            vault_state: f.vault_state,
            vault: f.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut f.svm, &attacker, ix).is_err());
    assert_eq!(
        f.svm.get_balance(&f.vault).unwrap(),
        balance,
        "the victim's vault must not change"
    );
}

#[test]
fn test_close_returns_everything_to_user() {
    let mut f = setup();
    let before = f.svm.get_balance(&f.user.pubkey()).unwrap();

    let ix = ix_initialize(&f);
    send(&mut f.svm, &f.user, ix).unwrap();
    let ix = ix_deposit(&f, DEPOSIT_LAMPORTS);
    send(&mut f.svm, &f.user, ix).unwrap();
    let ix = ix_close(&f);
    send(&mut f.svm, &f.user, ix).unwrap();

    // The user only lost transaction fees (3 transactions), so the deposit,
    // the vault rent and the state rent must all have come back
    let after = f.svm.get_balance(&f.user.pubkey()).unwrap();
    assert!(
        after <= before && before - after <= 3 * MAX_FEE,
        "closing should return the deposit, the vault rent and the state rent"
    );
    assert!(is_closed(&f.svm, &f.vault_state));
    assert!(is_closed(&f.svm, &f.vault));
}

#[test]
fn test_withdraw_after_close_fails() {
    let mut f = setup_with_deposit();
    let ix = ix_close(&f);
    send(&mut f.svm, &f.user, ix).unwrap();

    let ix = ix_withdraw(&f, WITHDRAW_LAMPORTS);
    assert!(send(&mut f.svm, &f.user, ix).is_err());
}
