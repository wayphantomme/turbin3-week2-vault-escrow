# Turbin3 Builders Cohort Q4 2026 — Week 2: Vault & Escrow

Two Anchor programs in one workspace, tested with [LiteSVM](https://github.com/LiteSVM/litesvm).

| Program | Path | What it does |
|---|---|---|
| **Vault** | [`programs/q3_26_vault`](programs/q3_26_vault) | A personal SOL vault per user: `initialize`, `deposit`, `withdraw`, `close` |
| **Escrow** | [`programs/escrowq32026`](programs/escrowq32026) | A trustless token swap with a deadline: `make`, `take`, `refund`, `update` |

## Assignment checklist

- [x] Vault with `withdraw` and `close`
- [x] Escrow with `make`, `take`, `refund`, `update`
- [x] Tests for every instruction, including failure cases (LiteSVM, Rust)
- [x] Optional: timed escrow using the Clock sysvar (`expiration`)
- [ ] Screenshot of all tests passing: `docs/tests-passing.png`
- [ ] Social post: _add link here_

## Quick start

Requirements: Rust 1.89.0 (pinned in `rust-toolchain.toml`), Solana CLI and Anchor CLI.

```bash
git clone https://github.com/wayphantomme/turbin3-week2-vault-escrow
cd turbin3-week2-vault-escrow
anchor keys sync                 # only needed if program ids do not match your local keypairs
cargo build-sbf --arch v0        # builds both programs into target/deploy
cargo test                       # runs the LiteSVM tests of both programs
```

> The tests load the compiled `.so` files from `target/deploy`, so run `cargo build-sbf` before `cargo test`
> (and again after every change to a program).

## Vault

Every user gets two PDAs derived from their wallet:

| Account | Seeds | Purpose |
|---|---|---|
| `vault_state` | `["state", user]` | Stores the two bumps |
| `vault` | `["vault", user]` | A system account that holds the SOL |

| Instruction | Behaviour |
|---|---|
| `initialize` | Creates `vault_state` and funds the vault with the rent-exempt minimum |
| `deposit(amount)` | Transfers SOL from the user to the vault. `amount` must be > 0 |
| `withdraw(amount)` | The vault PDA signs a transfer back to the user. Fails if `amount` is 0 or if the vault would drop below the rent-exempt minimum |
| `close` | Sends every lamport in the vault back to the user and closes `vault_state` (its rent goes back to the user too) |

Only the owner can touch a vault: all seeds include the signer's key, so another wallet cannot derive the same accounts.

## Escrow

A maker locks token A and asks for an amount of token B. Any taker holding token B can complete the swap atomically.

```
make    maker --token A--> vault (PDA-owned)
take    taker --token B--> maker        vault --token A--> taker      (vault + escrow closed)
refund  vault --token A--> maker                                      (vault + escrow closed)
update  maker changes `receive` and/or `expiration` while the escrow is open
```

| Instruction | Discriminator | Behaviour |
|---|---|---|
| `make(seed, deposit, receive, expiration)` | 0 | Creates the escrow and vault, deposits token A. `deposit` and `receive` must be > 0 and `expiration` must be in the future |
| `take` | 1 | Fails after `expiration`. The taker pays `receive` token B to the maker, the escrow PDA signs to send all token A to the taker, then the vault and escrow are closed. Missing token accounts are created |
| `refund` | 2 | Only the maker. Returns token A and closes the vault and escrow. Allowed at any time, also after expiration |
| `update(new_receive?, new_expiration?)` | 3 | Only the maker. Both fields are optional. `new_receive` must be > 0 and `new_expiration` must be in the future |

Accounts and custom 1-byte discriminators:
- `Escrow` PDA: `["escrow", maker, seed (u64 LE)]`, account discriminator `1`.
- The vault is the escrow's associated token account for token A.
- Time comes from `Clock::get()?.unix_timestamp`, which is an `i64`, so `expiration` is an `i64` too.

## Tests

`cargo test` runs:

**Vault** (`programs/q3_26_vault/tests`)
- full flow: initialize, deposit, withdraw, close
- deposit of 0 fails, withdraw of 0 fails
- withdraw more than the balance fails
- withdraw cannot break rent exemption (and withdrawing exactly the deposit works)
- another user cannot withdraw from someone else's vault
- close returns deposit and rent to the user
- withdraw after close fails

**Escrow** (`programs/escrowq32026/tests`)
- make and refund
- make and take (balances of both sides, accounts closed)
- take after expiration fails (clock moved with `set_sysvar`) and the maker can still refund
- take after refund fails, take twice fails
- update by the maker (both fields, one field, take uses the new price)
- update by a non-maker fails, update with invalid values fails
- make with invalid values fails

## What I learned

_Fill in your own words before submitting: PDAs as signers, custom discriminators, the Clock sysvar, and testing failure cases._
