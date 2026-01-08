//! Withdraw Instruction
//!
//! Authority withdraws tokens from the pool, decreasing liquidity.
//! Supports both SPL Token and Token-2022 tokens.

use pinocchio::{
    account_info::AccountInfo, instruction::Signer, program_error::ProgramError, pubkey::Pubkey,
    seeds, ProgramResult,
};

use crate::{
    error::PropAmmError,
    pda::POOL_SEED,
    state::Pool,
    token::{get_mint_decimals, transfer_tokens},
};

use super::deposit::TokenSide;

/// Withdraw instruction data layout
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WithdrawData {
    /// Which token to withdraw
    pub side: TokenSide,
    /// Amount to withdraw (native token units)
    pub amount: u64,
}

impl WithdrawData {
    pub const SIZE: usize = 1 + 8; // 9 bytes

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        let side = TokenSide::try_from_u8(data[0])?;
        let amount = u64::from_le_bytes(data[1..9].try_into().ok()?);

        Some(Self { side, amount })
    }
}

/// Process Withdraw instruction
///
/// Accounts:
/// 0. `[signer]` Authority - Must match pool.authority
/// 1. `[writable]` Pool - Pool account
/// 2. `[writable]` Authority Token Account - Authority's token account (destination)
/// 3. `[writable]` Pool Vault - Pool's token account (source)
/// 4. `[]` Mint - Token mint
/// 5. `[]` Token Program - SPL Token or Token-2022
pub fn process_withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // Parse instruction data
    let ix_data = WithdrawData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

    // Validate accounts
    if accounts.len() < 6 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let pool_account = &accounts[1];
    let authority_token_account = &accounts[2];
    let pool_vault = &accounts[3];
    let mint = &accounts[4];
    let token_program = &accounts[5];

    // Authority must be signer
    if !authority.is_signer() {
        return Err(PropAmmError::NotSigner.into());
    }

    // Validate writable accounts
    if !pool_account.is_writable()
        || !authority_token_account.is_writable()
        || !pool_vault.is_writable()
    {
        return Err(PropAmmError::NotWritable.into());
    }

    // Pool must be owned by this program
    if unsafe { pool_account.owner() } != program_id {
        return Err(PropAmmError::InvalidOwner.into());
    }

    // Validate amount
    if ix_data.amount == 0 {
        return Err(PropAmmError::ZeroAmount.into());
    }

    // Load pool
    let pool_data = pool_account.try_borrow_data()?;
    if pool_data.len() < Pool::SIZE {
        return Err(PropAmmError::AccountDataTooSmall.into());
    }

    let mut pool = Pool::from_bytes(&pool_data).ok_or(PropAmmError::InvalidDiscriminator)?;

    if !pool.is_valid_discriminator() {
        return Err(PropAmmError::InvalidDiscriminator.into());
    }

    // Verify authority
    if authority.key() != &pool.authority {
        return Err(PropAmmError::Unauthorized.into());
    }

    // Validate mint and vault match the side, check sufficient withdrawable quantity
    // Can only withdraw total_quantity - consumed (consumed represents traded liquidity)
    match ix_data.side {
        TokenSide::Base => {
            if mint.key() != &pool.base_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if pool_vault.key() != &pool.base_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
            // Check that we have enough withdrawable liquidity
            let withdrawable = pool.ask_side.total_quantity.saturating_sub(pool.ask_side.consumed);
            if withdrawable < ix_data.amount {
                return Err(PropAmmError::InsufficientFunds.into());
            }
        }
        TokenSide::Quote => {
            if mint.key() != &pool.quote_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if pool_vault.key() != &pool.quote_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
            // Check that we have enough withdrawable liquidity
            let withdrawable = pool.bid_side.total_quantity.saturating_sub(pool.bid_side.consumed);
            if withdrawable < ix_data.amount {
                return Err(PropAmmError::InsufficientFunds.into());
            }
        }
    }

    // Get decimals for transfer_checked
    let decimals = get_mint_decimals(mint)?;

    drop(pool_data);

    // Update pool state - decrease total_quantity
    match ix_data.side {
        TokenSide::Base => {
            pool.ask_side.total_quantity = pool
                .ask_side
                .total_quantity
                .checked_sub(ix_data.amount)
                .ok_or(PropAmmError::MathOverflow)?;
        }
        TokenSide::Quote => {
            pool.bid_side.total_quantity = pool
                .bid_side
                .total_quantity
                .checked_sub(ix_data.amount)
                .ok_or(PropAmmError::MathOverflow)?;
        }
    }

    // Write updated pool state
    let pool_bytes = pool.to_bytes();
    let mut pool_data = pool_account.try_borrow_mut_data()?;
    pool_data[..Pool::SIZE].copy_from_slice(&pool_bytes);
    drop(pool_data);

    // Transfer tokens from vault to authority (requires PDA signing)
    let bump_seed = [pool.bump];
    let pool_signer_seeds = seeds!(
        POOL_SEED,
        pool.base_mint.as_ref(),
        pool.quote_mint.as_ref(),
        &bump_seed
    );

    transfer_tokens(
        pool_vault,
        authority_token_account,
        pool_account,
        mint,
        token_program,
        ix_data.amount,
        decimals,
        &[Signer::from(&pool_signer_seeds)],
    )?;

    Ok(())
}
