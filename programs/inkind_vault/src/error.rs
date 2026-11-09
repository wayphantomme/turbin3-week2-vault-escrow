use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("First deposit must be larger than the locked minimum shares")]
    DepositTooSmall,
    #[msg("Required deposit exceeds your max_base / max_position slippage limits")]
    SlippageExceeded,
    #[msg("Base mint and position mint must be different")]
    SameMint,
    #[msg("Math overflow")]
    MathOverflow,
}
