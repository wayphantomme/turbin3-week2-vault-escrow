use crate::{
    constants::{SHARES_SEED, VAULT_SEED},
    error::VaultError,
    state::Vault,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Initialize<'info> {
    // Pays for account creation rent. Receives no special rights afterwards.
    #[account(mut)]
    pub payer: Signer<'info>,

    pub base_mint: InterfaceAccount<'info, Mint>,
    pub position_mint: InterfaceAccount<'info, Mint>,

    // One vault per (base_mint, position_mint) pair. Using both mints as seeds
    // guarantees that each pair can only ever have one canonical vault.
    #[account(
        init,
        payer = payer,
        space = Vault::DISCRIMINATOR.len() + Vault::INIT_SPACE,
        seeds = [VAULT_SEED, base_mint.key().as_ref(), position_mint.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,

    // The mint authority is the vault PDA — no wallet controls share issuance.
    #[account(
        init,
        payer = payer,
        seeds = [SHARES_SEED, vault.key().as_ref()],
        bump,
        mint::decimals = base_mint.decimals,
        mint::authority = vault,
        mint::token_program = token_program
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    // Vault-owned ATAs that hold the pooled assets.
    #[account(
        init,
        payer = payer,
        associated_token::mint = base_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub base_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = position_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program
    )]
    pub position_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn initialize(&mut self, bumps: &InitializeBumps) -> Result<()> {
        // If both mints were the same, base_vault and position_vault would
        // resolve to the same address and the second `init` would fail anyway,
        // but rejecting it here gives a clearer error.
        require_keys_neq!(
            self.base_mint.key(),
            self.position_mint.key(),
            VaultError::SameMint
        );

        self.vault.base_mint = self.base_mint.key();
        self.vault.position_mint = self.position_mint.key();
        self.vault.share_mint = self.share_mint.key();
        self.vault.bump = bumps.vault;
        Ok(())
    }
}
