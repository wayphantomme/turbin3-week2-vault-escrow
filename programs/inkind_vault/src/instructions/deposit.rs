use crate::{
    constants::{MINIMUM_SHARES, VAULT_SEED},
    error::VaultError,
    state::Vault,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked},
};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [VAULT_SEED, vault.base_mint.as_ref(), vault.position_mint.as_ref()],
        bump = vault.bump,
        has_one = base_mint,
        has_one = position_mint,
        has_one = share_mint
    )]
    pub vault: Box<Account<'info, Vault>>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,
    pub position_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = base_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub position_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    // ATA owned by the vault PDA itself. MINIMUM_SHARES are minted here on the
    // first deposit and can never be burned because the vault PDA has no private
    // key — no one can sign a burn instruction for this account.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = share_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub vault_shares: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = base_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_base: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = position_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_position: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = share_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_shares: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// Integer division rounded UP. Used for required deposit amounts so the
// vault always receives at least its fair share (rounding favors the pool).
fn ceil_div(a: u128, b: u128) -> Result<u128> {
    let numerator = a
        .checked_add(
            b.checked_sub(1)
                .ok_or_else(|| error!(VaultError::MathOverflow))?,
        )
        .ok_or_else(|| error!(VaultError::MathOverflow))?;
    numerator
        .checked_div(b)
        .ok_or_else(|| error!(VaultError::MathOverflow))
}

fn to_u64(v: u128) -> Result<u64> {
    u64::try_from(v).map_err(|_| error!(VaultError::MathOverflow))
}

impl<'info> Deposit<'info> {
    // ── First deposit ────────────────────────────────────────────────────────
    // The caller sets the initial ratio by choosing max_base and max_position.
    // MINIMUM_SHARES are minted to vault_shares (the vault's own ATA) and
    // locked forever — nobody can sign a burn for that account. The user
    // receives max_base shares on top of that, so supply = max_base + MINIMUM_SHARES.
    // This makes the share-price inflation attack uneconomic: an attacker would
    // have to donate at least MINIMUM_SHARES worth of base tokens.
    //
    // ── Subsequent deposits ──────────────────────────────────────────────────
    // The caller asks for `shares`. The program computes how much base and
    // position token must be transferred in (rounded up). No oracle or external
    // price feed is involved, so the share price cannot be manipulated.
    //
    // ── Slippage guard ───────────────────────────────────────────────────────
    // The instruction rejects the transaction if the required deposit exceeds
    // max_base or max_position, giving callers frontrun protection.
    pub fn deposit(&mut self, shares: u64, max_base: u64, max_position: u64) -> Result<()> {
        let supply = self.share_mint.supply as u128;
        let base_bal = self.base_vault.amount as u128;
        let position_bal = self.position_vault.amount as u128;

        let base_key = self.vault.base_mint;
        let position_key = self.vault.position_mint;
        let bump = [self.vault.bump];
        let signer_seeds: [&[&[u8]]; 1] =
            [&[VAULT_SEED, base_key.as_ref(), position_key.as_ref(), &bump]];

        let (base_in, position_in, shares_out) = if supply == 0 {
            // First deposit: at least MINIMUM_SHARES must be affordable
            require!(max_base > 0, VaultError::DepositTooSmall);

            // Mint MINIMUM_SHARES to the vault's own share ATA — locked forever
            mint_to(
                CpiContext::new_with_signer(
                    self.token_program.key(),
                    MintTo {
                        mint: self.share_mint.to_account_info(),
                        to: self.vault_shares.to_account_info(),
                        authority: self.vault.to_account_info(),
                    },
                    &signer_seeds,
                ),
                MINIMUM_SHARES,
            )?;

            // The user receives max_base shares on top of the locked portion
            (max_base, max_position, max_base)
        } else {
            require!(shares > 0, VaultError::InvalidAmount);
            let s = shares as u128;
            // Required base = ceil(shares * base_balance / supply)
            let base_in = ceil_div(
                s.checked_mul(base_bal)
                    .ok_or_else(|| error!(VaultError::MathOverflow))?,
                supply,
            )?;
            // Required position = ceil(shares * position_balance / supply)
            let position_in = ceil_div(
                s.checked_mul(position_bal)
                    .ok_or_else(|| error!(VaultError::MathOverflow))?,
                supply,
            )?;
            (to_u64(base_in)?, to_u64(position_in)?, shares)
        };

        // Reject if the required amounts exceed the caller's slippage limits
        require!(
            base_in <= max_base && position_in <= max_position,
            VaultError::SlippageExceeded
        );

        // User signs both transfers into the vault
        transfer_checked(
            CpiContext::new(
                self.token_program.key(),
                TransferChecked {
                    from: self.user_base.to_account_info(),
                    mint: self.base_mint.to_account_info(),
                    to: self.base_vault.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            base_in,
            self.base_mint.decimals,
        )?;

        transfer_checked(
            CpiContext::new(
                self.token_program.key(),
                TransferChecked {
                    from: self.user_position.to_account_info(),
                    mint: self.position_mint.to_account_info(),
                    to: self.position_vault.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            position_in,
            self.position_mint.decimals,
        )?;

        // Mint the user's shares
        mint_to(
            CpiContext::new_with_signer(
                self.token_program.key(),
                MintTo {
                    mint: self.share_mint.to_account_info(),
                    to: self.user_shares.to_account_info(),
                    authority: self.vault.to_account_info(),
                },
                &signer_seeds,
            ),
            shares_out,
        )
    }
}
