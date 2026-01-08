//! SetVaults Instruction
//!
//! Sets the base and quote vault addresses for the pool.
//! This is a one-time setup after Initialize.

use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult};

use crate::{error::PropAmmError, state::Pool};

/// Process SetVaults instruction
///
/// Accounts:
/// 0. `[signer]` Authority - Must match pool authority
/// 1. `[writable]` Pool - Pool account
/// 2. `[]` Base Vault - Token account for base token (wSOL)
/// 3. `[]` Quote Vault - Token account for quote token (USDC)
pub fn process_set_vaults(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _data: &[u8],
) -> ProgramResult {
    // Validate accounts
    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let pool_account = &accounts[1];
    let base_vault = &accounts[2];
    let quote_vault = &accounts[3];

    // Authority must be signer
    if !authority.is_signer() {
        return Err(PropAmmError::NotSigner.into());
    }

    // Pool must be writable
    if !pool_account.is_writable() {
        return Err(PropAmmError::NotWritable.into());
    }

    // Pool must be owned by this program
    // SAFETY: We are only reading the owner field to validate
    if unsafe { pool_account.owner() } != program_id {
        return Err(PropAmmError::InvalidOwner.into());
    }

    // Load pool to validate authority and check vaults aren't already set
    let pool_data = pool_account.try_borrow_data()?;
    if pool_data.len() < Pool::SIZE {
        return Err(PropAmmError::AccountDataTooSmall.into());
    }

    let pool = Pool::from_bytes(&pool_data).ok_or(PropAmmError::InvalidDiscriminator)?;

    if !pool.is_valid_discriminator() {
        return Err(PropAmmError::InvalidDiscriminator.into());
    }

    // Verify authority matches
    if authority.key() != &pool.authority {
        return Err(PropAmmError::Unauthorized.into());
    }

    // Check vaults aren't already set (all zeros means not set)
    let zero_key = [0u8; 32];
    if pool.base_vault != zero_key || pool.quote_vault != zero_key {
        // Vaults already set - could add an error for this
        return Err(PropAmmError::InvalidTokenAccount.into());
    }

    drop(pool_data);

    // Write vault addresses directly to pool account data
    // base_vault is at offset 105, quote_vault is at offset 137
    let mut pool_data = pool_account.try_borrow_mut_data()?;
    pool_data[105..137].copy_from_slice(base_vault.key().as_ref());
    pool_data[137..169].copy_from_slice(quote_vault.key().as_ref());

    Ok(())
}
