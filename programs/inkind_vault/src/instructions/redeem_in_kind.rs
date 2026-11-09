use crate::{constants::VAULT_SEED, error::VaultError, state::Vault};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

#[derive(Accounts)]
pub struct RedeemInKind<'info> {
    // The user signs the burn; only the share-token owner can trigger a redemption.
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [VAULT_SEED, vault.base_mint.as_ref(), vault.position_mint.as_ref()],
        bump = vault.bump,
        has_one = base_mint,
        has_one = position_mint,
        has_one = share_mint
    )]
    pub vault: Account<'info, Vault>,

    pub base_mint: InterfaceAccount<'info, Mint>,
    pub position_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = base_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub base_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub position_vault: InterfaceAccount<'info, TokenAccount>,

    // `init_if_needed`: a user who never held these tokens before must still be
    // able to exit. Failing on a missing ATA would effectively lock them in.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = base_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_base: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = position_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_position: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = share_mint,
        token::authority = user,
        token::token_program = token_program
    )]
    pub user_shares: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> RedeemInKind<'info> {
    // Burns `shares` and sends the caller a pro-rata portion of BOTH vault assets.
    //
    // Key properties:
    //   • No liquidity check — if base_vault is empty the user still receives
    //     their share of position tokens and can unwind the market position
    //     themselves when liquidity is available again.
    //   • No admin, pause, lockup, or price feed — pure on-chain arithmetic.
    //   • Outputs are rounded DOWN (floor), which is standard and safe for the vault.
    //   • Burn happens before transfers, so a re-entrant call would see a reduced
    //     supply and compute smaller outputs.
    pub fn redeem_in_kind(&mut self, shares: u64) -> Result<()> {
        require!(shares > 0, VaultError::InvalidAmount);

        let supply = self.share_mint.supply as u128;
        // Guard against a pathological state where supply is 0 but shares exist
        require!(supply > 0, VaultError::InvalidAmount);

        let s = shares as u128;

        // base_out  = floor(shares * base_balance  / supply)
        let base_out = u64::try_from(
            s.checked_mul(self.base_vault.amount as u128)
                .ok_or_else(|| error!(VaultError::MathOverflow))?
                / supply,
        )
        .map_err(|_| error!(VaultError::MathOverflow))?;

        // position_out = floor(shares * position_balance / supply)
        let position_out = u64::try_from(
            s.checked_mul(self.position_vault.amount as u128)
                .ok_or_else(|| error!(VaultError::MathOverflow))?
                / supply,
        )
        .map_err(|_| error!(VaultError::MathOverflow))?;

        // ── Step 1: burn shares ───────────────────────────────────────────────
        // The user signs this CPI, so only the actual share holder can redeem.
        burn(
            CpiContext::new(
                self.token_program.key(),
                Burn {
                    mint: self.share_mint.to_account_info(),
                    from: self.user_shares.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            shares,
        )?;

        // ── Step 2: vault PDA signs the outgoing transfers ────────────────────
        let base_key = self.vault.base_mint;
        let position_key = self.vault.position_mint;
        let bump = [self.vault.bump];
        let signer_seeds: [&[&[u8]]; 1] =
            [&[VAULT_SEED, base_key.as_ref(), position_key.as_ref(), &bump]];

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.key(),
                TransferChecked {
                    from: self.base_vault.to_account_info(),
                    mint: self.base_mint.to_account_info(),
                    to: self.user_base.to_account_info(),
                    authority: self.vault.to_account_info(),
                },
                &signer_seeds,
            ),
            base_out,
            self.base_mint.decimals,
        )?;

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.key(),
                TransferChecked {
                    from: self.position_vault.to_account_info(),
                    mint: self.position_mint.to_account_info(),
                    to: self.user_position.to_account_info(),
                    authority: self.vault.to_account_info(),
                },
                &signer_seeds,
            ),
            position_out,
            self.position_mint.decimals,
        )
    }
}
