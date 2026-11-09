use {
    anchor_lang::{
        solana_program::{
            clock::Clock, instruction::Instruction, program_pack::Pack,
            system_program::ID as SYSTEM_PROGRAM_ID,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::spl_token,
    },
    litesvm::LiteSVM,
    litesvm_token::{
        spl_token::ID as TOKEN_PROGRAM_ID, CreateAssociatedTokenAccount, CreateMint, MintTo,
    },
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::Transaction,
};

const SEED: u64 = 123;
const DEPOSIT: u64 = 10_000_000; // token A locked in the vault by the maker
const RECEIVE: u64 = 20_000_000; // token B the maker asks for
const NOW: i64 = 1_800_000_000; // "current time" used by every test
const FAR_FUTURE: i64 = NOW + 1_000_000; // an expiration that is far away

// Everything a test needs: the SVM, both users, the mints and every derived address
struct Ctx {
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    maker_ata_a: Pubkey,
    taker_ata_b: Pubkey,
    taker_ata_a: Pubkey,
    maker_ata_b: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
}

fn set_time(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

// Sends one instruction and returns Ok(()) or the error text with the program logs
fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> Result<(), String> {
    // A fresh blockhash lets us send the same instruction twice without "AlreadyProcessed"
    svm.expire_blockhash();
    let message = Message::new(&[ix], Some(&payer.pubkey()));
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new(&[payer], message, blockhash);
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|err| format!("{:?}\nlogs: {:#?}", err.err, err.meta.logs))
}

// Creates the mints, the token accounts and the balances used by all the tests:
// maker holds 1,000 token A and taker holds 1,000 token B
fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/escrowq32026.so"
    ));
    svm.add_program(escrowq32026::id(), bytes).unwrap();
    set_time(&mut svm, NOW);

    let maker = Keypair::new();
    let taker = Keypair::new();
    svm.airdrop(&maker.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&taker.pubkey(), 10_000_000_000).unwrap();
    let maker_pk = maker.pubkey();
    let taker_pk = taker.pubkey();

    let mint_a = CreateMint::new(&mut svm, &maker)
        .decimals(6)
        .authority(&maker_pk)
        .send()
        .unwrap();
    let mint_b = CreateMint::new(&mut svm, &maker)
        .decimals(6)
        .authority(&maker_pk)
        .send()
        .unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
        .owner(&maker_pk)
        .send()
        .unwrap();
    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_b)
        .owner(&taker_pk)
        .send()
        .unwrap();

    MintTo::new(&mut svm, &maker, &mint_a, &maker_ata_a, 1_000_000_000)
        .send()
        .unwrap();
    MintTo::new(&mut svm, &maker, &mint_b, &taker_ata_b, 1_000_000_000)
        .send()
        .unwrap();

    let escrow = Pubkey::find_program_address(
        &[b"escrow", maker_pk.as_ref(), &SEED.to_le_bytes()],
        &escrowq32026::id(),
    )
    .0;
    let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
    // These two accounts do not exist yet: "take" creates them when needed
    let taker_ata_a = associated_token::get_associated_token_address(&taker_pk, &mint_a);
    let maker_ata_b = associated_token::get_associated_token_address(&maker_pk, &mint_b);

    Ctx {
        svm,
        maker,
        taker,
        mint_a,
        mint_b,
        maker_ata_a,
        taker_ata_b,
        taker_ata_a,
        maker_ata_b,
        escrow,
        vault,
    }
}

fn make_ix(c: &Ctx, deposit: u64, receive: u64, expiration: i64) -> Instruction {
    Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Make {
            maker: c.maker.pubkey(),
            mint_a: c.mint_a,
            mint_b: c.mint_b,
            maker_ata_a: c.maker_ata_a,
            escrow: c.escrow,
            vault: c.vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Make {
            seed: SEED,
            deposit,
            receive,
            expiration,
        }
        .data(),
    }
}

fn take_ix(c: &Ctx) -> Instruction {
    Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Take {
            taker: c.taker.pubkey(),
            maker: c.maker.pubkey(),
            mint_a: c.mint_a,
            mint_b: c.mint_b,
            taker_ata_a: c.taker_ata_a,
            taker_ata_b: c.taker_ata_b,
            maker_ata_b: c.maker_ata_b,
            escrow: c.escrow,
            vault: c.vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Take {}.data(),
    }
}

fn refund_ix(c: &Ctx) -> Instruction {
    Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Refund {
            maker: c.maker.pubkey(),
            mint_a: c.mint_a,
            maker_ata_a: c.maker_ata_a,
            escrow: c.escrow,
            vault: c.vault,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Refund {}.data(),
    }
}

// `signer` is the account that signs as "maker": the real maker, or an attacker
fn update_ix(
    c: &Ctx,
    signer: &Pubkey,
    new_receive: Option<u64>,
    new_expiration: Option<i64>,
) -> Instruction {
    Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Update {
            maker: *signer,
            escrow: c.escrow,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Update {
            new_receive,
            new_expiration,
        }
        .data(),
    }
}

fn token_balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    spl_token::state::Account::unpack(&account.data)
        .unwrap()
        .amount
}

fn is_closed(svm: &LiteSVM, address: &Pubkey) -> bool {
    svm.get_account(address)
        .map(|account| account.lamports == 0)
        .unwrap_or(true)
}

fn read_escrow(svm: &LiteSVM, address: &Pubkey) -> escrowq32026::state::Escrow {
    let account = svm.get_account(address).unwrap();
    escrowq32026::state::Escrow::try_deserialize(&mut account.data.as_ref()).unwrap()
}

#[test]
fn test_make_and_refund() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();

    // make: tokens move to the vault and the escrow stores the terms
    assert_eq!(token_balance(&c.svm, &c.vault), DEPOSIT);
    assert_eq!(token_balance(&c.svm, &c.maker_ata_a), 1_000_000_000 - DEPOSIT);
    let escrow = read_escrow(&c.svm, &c.escrow);
    assert_eq!(escrow.seed, SEED);
    assert_eq!(escrow.maker, c.maker.pubkey());
    assert_eq!(escrow.mint_a, c.mint_a);
    assert_eq!(escrow.mint_b, c.mint_b);
    assert_eq!(escrow.receive, RECEIVE);
    assert_eq!(escrow.expiration, FAR_FUTURE);

    // refund: tokens return to the maker and both accounts are closed
    let ix = refund_ix(&c);
    send(&mut c.svm, &c.maker, ix).unwrap();
    assert_eq!(token_balance(&c.svm, &c.maker_ata_a), 1_000_000_000);
    assert!(is_closed(&c.svm, &c.vault));
    assert!(is_closed(&c.svm, &c.escrow));
}

#[test]
fn test_make_and_take() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();

    let ix = take_ix(&c);
    send(&mut c.svm, &c.taker, ix).unwrap();

    // the taker received token A and paid token B, the maker received token B
    assert_eq!(token_balance(&c.svm, &c.taker_ata_a), DEPOSIT);
    assert_eq!(token_balance(&c.svm, &c.taker_ata_b), 1_000_000_000 - RECEIVE);
    assert_eq!(token_balance(&c.svm, &c.maker_ata_b), RECEIVE);
    // the maker did not get token A back
    assert_eq!(token_balance(&c.svm, &c.maker_ata_a), 1_000_000_000 - DEPOSIT);
    // vault and escrow are closed
    assert!(is_closed(&c.svm, &c.vault));
    assert!(is_closed(&c.svm, &c.escrow));
}

#[test]
fn test_take_after_expiration_fails() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, NOW + 100);
    send(&mut c.svm, &c.maker, ix).unwrap();

    // move the clock past the expiration time
    set_time(&mut c.svm, NOW + 200);
    let ix = take_ix(&c);
    let err = send(&mut c.svm, &c.taker, ix).unwrap_err();
    assert!(err.contains("EscrowExpired"), "unexpected error: {err}");
    assert_eq!(token_balance(&c.svm, &c.vault), DEPOSIT, "vault must be untouched");

    // the maker can still get the tokens back with refund
    let ix = refund_ix(&c);
    send(&mut c.svm, &c.maker, ix).unwrap();
    assert_eq!(token_balance(&c.svm, &c.maker_ata_a), 1_000_000_000);
}

#[test]
fn test_take_after_refund_fails() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();
    let ix = refund_ix(&c);
    send(&mut c.svm, &c.maker, ix).unwrap();

    let ix = take_ix(&c);
    assert!(send(&mut c.svm, &c.taker, ix).is_err());
}

#[test]
fn test_take_twice_fails() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();
    let ix = take_ix(&c);
    send(&mut c.svm, &c.taker, ix).unwrap();

    let ix = take_ix(&c);
    assert!(send(&mut c.svm, &c.taker, ix).is_err());
}

#[test]
fn test_update_by_maker() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();

    // change both fields
    let maker_pk = c.maker.pubkey();
    let ix = update_ix(&c, &maker_pk, Some(30_000_000), Some(FAR_FUTURE + 5_000));
    send(&mut c.svm, &c.maker, ix).unwrap();
    let escrow = read_escrow(&c.svm, &c.escrow);
    assert_eq!(escrow.receive, 30_000_000);
    assert_eq!(escrow.expiration, FAR_FUTURE + 5_000);

    // change only the price: the expiration stays as it was
    let ix = update_ix(&c, &maker_pk, Some(40_000_000), None);
    send(&mut c.svm, &c.maker, ix).unwrap();
    let escrow = read_escrow(&c.svm, &c.escrow);
    assert_eq!(escrow.receive, 40_000_000);
    assert_eq!(escrow.expiration, FAR_FUTURE + 5_000);

    // take now charges the updated price
    let ix = take_ix(&c);
    send(&mut c.svm, &c.taker, ix).unwrap();
    assert_eq!(token_balance(&c.svm, &c.maker_ata_b), 40_000_000);
}

#[test]
fn test_update_by_non_maker_fails() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();

    // the taker signs and claims to be the maker
    let taker_pk = c.taker.pubkey();
    let ix = update_ix(&c, &taker_pk, Some(1), None);
    assert!(send(&mut c.svm, &c.taker, ix).is_err());
    assert_eq!(read_escrow(&c.svm, &c.escrow).receive, RECEIVE);
}

#[test]
fn test_update_with_invalid_values_fails() {
    let mut c = setup();

    let ix = make_ix(&c, DEPOSIT, RECEIVE, FAR_FUTURE);
    send(&mut c.svm, &c.maker, ix).unwrap();
    let maker_pk = c.maker.pubkey();

    let ix = update_ix(&c, &maker_pk, Some(0), None);
    let err = send(&mut c.svm, &c.maker, ix).unwrap_err();
    assert!(err.contains("InvalidAmount"), "unexpected error: {err}");

    let ix = update_ix(&c, &maker_pk, None, Some(NOW - 1));
    let err = send(&mut c.svm, &c.maker, ix).unwrap_err();
    assert!(err.contains("InvalidExpiration"), "unexpected error: {err}");

    let escrow = read_escrow(&c.svm, &c.escrow);
    assert_eq!(escrow.receive, RECEIVE);
    assert_eq!(escrow.expiration, FAR_FUTURE);
}

#[test]
fn test_make_with_invalid_values_fails() {
    let mut c = setup();

    let ix = make_ix(&c, 0, RECEIVE, FAR_FUTURE);
    let err = send(&mut c.svm, &c.maker, ix).unwrap_err();
    assert!(err.contains("InvalidAmount"), "unexpected error: {err}");

    let ix = make_ix(&c, DEPOSIT, 0, FAR_FUTURE);
    let err = send(&mut c.svm, &c.maker, ix).unwrap_err();
    assert!(err.contains("InvalidAmount"), "unexpected error: {err}");

    let ix = make_ix(&c, DEPOSIT, RECEIVE, NOW - 1);
    let err = send(&mut c.svm, &c.maker, ix).unwrap_err();
    assert!(err.contains("InvalidExpiration"), "unexpected error: {err}");

    assert!(is_closed(&c.svm, &c.escrow), "no escrow should have been created");
}
