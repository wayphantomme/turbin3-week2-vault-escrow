use anchor_lang::prelude::*;

// Note: there is intentionally NO `authority` field.
// Nobody — including the original deployer — has a privileged role over
// user funds once the vault is initialized. The only signer that can move
// assets out of the vault token accounts is the vault PDA itself, and the
// program only lets it do so inside `redeem_in_kind` after burning shares.
#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub base_mint: Pubkey,     // liquid asset held by the vault (e.g. USDC)
    pub position_mint: Pubkey, // receipt token for the underlying market position
    pub share_mint: Pubkey,    // fungible claim on a pro-rata slice of both assets
    pub bump: u8,
}
