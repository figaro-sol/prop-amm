//! SwapEvent Instruction
//!
//! A no-op instruction used for emitting swap events via self-CPI.
//! The swap instruction invokes this after successful execution,
//! embedding trade data in the instruction that can be parsed from transactions.
//!
//! ## Data Layout (49 bytes after discriminator)
//! - user: Pubkey (32 bytes)
//! - direction: u8 (0=Buy, 1=Sell)
//! - amount_in: u64
//! - amount_out: u64

use pinocchio::{account_info::AccountInfo, pubkey::Pubkey, ProgramResult};

/// Process SwapEvent instruction
///
/// This is a no-op - it exists only so the instruction data
/// is recorded in the transaction for frontend parsing.
///
/// No accounts required.
pub fn process_swap_event(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _data: &[u8],
) -> ProgramResult {
    // No-op - the instruction data is already recorded in the transaction
    Ok(())
}
