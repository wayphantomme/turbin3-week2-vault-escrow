use anchor_lang::prelude::*;

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const SHARES_SEED: &[u8] = b"shares";

// Shares locked forever on the first deposit (never minted, never redeemable).
// This makes the share-price inflation attack on an empty vault too expensive
// to attempt: an attacker would have to donate at least MINIMUM_SHARES worth
// of base tokens and receive nothing back.
#[constant]
pub const MINIMUM_SHARES: u64 = 1_000;
