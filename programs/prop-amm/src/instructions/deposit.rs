//! Deposit Instruction
//!
//! Authority deposits tokens into the pool, increasing liquidity.
//! Supports both SPL Token and Token-2022 tokens.

use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult};

use crate::{
    error::PropAmmError,
    state::Pool,
    token::{get_mint_decimals, transfer_tokens},
};

/// Token side for deposit/withdraw
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenSide {
    /// Base token - affects ask_side.total_quantity
    Base = 0,
    /// Quote token - affects bid_side.total_quantity
    Quote = 1,
}

impl TokenSide {
    pub fn try_from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TokenSide::Base),
            1 => Some(TokenSide::Quote),
            _ => None,
        }
    }
}

/// Deposit instruction data layout
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DepositData {
    /// Which token to deposit
    pub side: TokenSide,
    /// Amount to deposit (native token units)
    pub amount: u64,
}

impl DepositData {
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

/// Process Deposit instruction
///
/// Accounts:
/// 0. `[signer]` Authority - Must match pool.authority
/// 1. `[writable]` Pool - Pool account
/// 2. `[writable]` Authority Token Account - Authority's token account (source)
/// 3. `[writable]` Pool Vault - Pool's token account (destination)
/// 4. `[]` Mint - Token mint
/// 5. `[]` Token Program - SPL Token or Token-2022
pub fn process_deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // Validate accounts
    if accounts.len() < 6 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // Parse instruction data
    let ix_data = DepositData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

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
    let pool_data = pool_account.try_borrow_data().map_err(|_| ProgramError::Custom(5001))?;
    if pool_data.len() < Pool::SIZE {
        return Err(ProgramError::Custom(5002));
    }

    let mut pool = Pool::from_bytes(&pool_data).ok_or(ProgramError::Custom(5003))?;

    if !pool.is_valid_discriminator() {
        return Err(ProgramError::Custom(5004));
    }

    // Verify authority
    if authority.key() != &pool.authority {
        return Err(ProgramError::Custom(5005));
    }

    // Validate mint and vault match the side
    match ix_data.side {
        TokenSide::Base => {
            if mint.key() != &pool.base_mint {
                return Err(ProgramError::Custom(5006));
            }
            if pool_vault.key() != &pool.base_vault {
                return Err(ProgramError::Custom(5007));
            }
        }
        TokenSide::Quote => {
            if mint.key() != &pool.quote_mint {
                return Err(ProgramError::Custom(5008));
            }
            if pool_vault.key() != &pool.quote_vault {
                return Err(ProgramError::Custom(5009));
            }
        }
    }

    // Get decimals for transfer_checked
    let decimals = get_mint_decimals(mint).map_err(|_| ProgramError::Custom(5010))?;

    drop(pool_data);

    // Update pool state - increase total_quantity
    match ix_data.side {
        TokenSide::Base => {
            pool.ask_side.total_quantity = pool
                .ask_side
                .total_quantity
                .checked_add(ix_data.amount)
                .ok_or(ProgramError::Custom(5011))?;
        }
        TokenSide::Quote => {
            pool.bid_side.total_quantity = pool
                .bid_side
                .total_quantity
                .checked_add(ix_data.amount)
                .ok_or(ProgramError::Custom(5012))?;
        }
    }

    // Write updated pool state
    let pool_bytes = pool.to_bytes();
    let mut pool_data = pool_account.try_borrow_mut_data().map_err(|_| ProgramError::Custom(5013))?;
    pool_data[..Pool::SIZE].copy_from_slice(&pool_bytes);
    drop(pool_data);

    // Transfer tokens from authority to vault
    transfer_tokens(
        authority_token_account,
        pool_vault,
        authority,
        mint,
        token_program,
        ix_data.amount,
        decimals,
        &[],
    ).map_err(|_| ProgramError::Custom(5014))?;

    Ok(())
}
