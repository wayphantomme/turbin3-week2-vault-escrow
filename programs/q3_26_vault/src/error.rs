use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Not enough funds: the vault must stay rent-exempt after a withdrawal")]
    InsufficientFunds,
}
