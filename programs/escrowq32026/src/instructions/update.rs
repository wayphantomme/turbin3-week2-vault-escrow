use anchor_lang::prelude::*;

use crate::{error::ErrorCode, state::Escrow, ESCROW_SEED};

// The maker can change the price (`receive`) and/or the deadline (`expiration`)
// of an escrow that has not been taken or refunded yet.
#[derive(Accounts)]
pub struct Update<'info> {
    pub maker: Signer<'info>,
    #[account(
        mut,
        has_one = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    pub fn update_escrow(
        &mut self,
        new_receive: Option<u64>,
        new_expiration: Option<i64>,
    ) -> Result<()> {
        if let Some(receive) = new_receive {
            require!(receive > 0, ErrorCode::InvalidAmount);
            self.escrow.receive = receive;
        }
        if let Some(expiration) = new_expiration {
            require!(
                expiration > Clock::get()?.unix_timestamp,
                ErrorCode::InvalidExpiration
            );
            self.escrow.expiration = expiration;
        }
        Ok(())
    }
}
