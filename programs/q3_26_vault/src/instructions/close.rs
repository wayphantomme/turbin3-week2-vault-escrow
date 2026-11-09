use crate::{
    constants::{STATE, VAULT_SEED},
    state::VaultState,
};
use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    // `close = user` sends the rent of the state account back to the user
    #[account(
        mut,
        seeds = [STATE, user.key().as_ref()],
        bump = vault_state.state_bump,
        close = user
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump = vault_state.vault_bump
    )]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> Close<'info> {
    // Moves every lamport left in the vault (deposits and rent) back to the user
    pub fn close(&mut self) -> Result<()> {
        let user_key = self.user.key();
        let bump = [self.vault_state.vault_bump];
        let signer_seeds: [&[&[u8]]; 1] = [&[VAULT_SEED, user_key.as_ref(), &bump]];

        let cpi_program = self.system_program.key();
        let cpi_accounts = Transfer {
            from: self.vault.to_account_info(),
            to: self.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, &signer_seeds);

        transfer(cpi_ctx, self.vault.lamports())
    }
}
