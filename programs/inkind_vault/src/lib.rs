pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

// Replace with the real program id after running `anchor keys sync`.
declare_id!("8wFAf6dxDpufjnjWXJcqC6AoptqYdCKRvDkrAuZYhDiT");

#[program]
pub mod inkind_vault {
    use super::*;

    // Creates the vault state account, the share mint (authority = vault PDA),
    // and the two vault-owned ATAs that will hold base and position tokens.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.initialize(&ctx.bumps)
    }

    // Deposit base + position tokens and receive shares in return.
    // `shares`       — how many shares the caller wants (ignored on first deposit)
    // `max_base`     — maximum base tokens the caller is willing to pay
    // `max_position` — maximum position tokens the caller is willing to pay
    pub fn deposit(
        ctx: Context<Deposit>,
        shares: u64,
        max_base: u64,
        max_position: u64,
    ) -> Result<()> {
        ctx.accounts.deposit(shares, max_base, max_position)
    }

    // Burn `shares` and receive a pro-rata slice of BOTH vault assets immediately,
    // regardless of whether the vault holds idle base liquidity.
    pub fn redeem_in_kind(ctx: Context<RedeemInKind>, shares: u64) -> Result<()> {
        ctx.accounts.redeem_in_kind(shares)
    }
}
