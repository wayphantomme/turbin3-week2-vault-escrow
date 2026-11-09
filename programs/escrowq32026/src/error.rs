use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Expiration must be in the future")]
    InvalidExpiration,
}
