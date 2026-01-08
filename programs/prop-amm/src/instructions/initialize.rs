//! Initialize Instruction
//!
//! Creates a new Prop AMM pool with initial parameters.

use pinocchio::{
    account_info::AccountInfo,
    instruction::Signer,
    program_error::ProgramError,
    pubkey::Pubkey,
    seeds,
    sysvars::Sysvar,
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    error::PropAmmError,
    pda::{create_pool_address_with_bump, POOL_SEED},
    state::{PiecewiseBookSide, Pool, POOL_DISCRIMINATOR},
};

/// Initialize instruction data layout
///
/// Simplified for piecewise AMM - oracle will set prices and quantities
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InitializeData {
    /// Pool PDA bump seed
    pub bump: u8,
}

impl InitializeData {
    pub const SIZE: usize = 1; // 1 byte

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        Some(Self { bump: data[0] })
    }
}

/// Process Initialize instruction
///
/// Accounts:
/// 0. `[signer]` Authority - Pool authority and payer
/// 1. `[writable]` Pool - Pool account (PDA, will be created)
/// 2. `[]` Base Mint - wSOL mint
/// 3. `[]` Quote Mint - USDC mint
/// 4. `[]` System Program
pub fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // Parse instruction data
    let ix_data =
        InitializeData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

    // Validate accounts
    if accounts.len() < 5 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let pool_account = &accounts[1];
    let base_mint = &accounts[2];
    let quote_mint = &accounts[3];
    let _system_program = &accounts[4];

    // Authority must be signer
    if !authority.is_signer() {
        return Err(PropAmmError::NotSigner.into());
    }

    // Pool must be writable
    if !pool_account.is_writable() {
        return Err(PropAmmError::NotWritable.into());
    }

    // Verify PDA derivation
    let expected_pool = create_pool_address_with_bump(
        program_id,
        base_mint.key(),
        quote_mint.key(),
        ix_data.bump,
    )
    .ok_or(PropAmmError::InvalidPda)?;

    if pool_account.key() != &expected_pool {
        return Err(PropAmmError::InvalidPda.into());
    }

    // Create pool account (321 bytes for piecewise AMM)
    let bump_seed = [ix_data.bump];
    let pool_seeds = seeds!(
        POOL_SEED,
        base_mint.key().as_ref(),
        quote_mint.key().as_ref(),
        &bump_seed
    );

    CreateAccount {
        from: authority,
        to: pool_account,
        lamports: pinocchio::sysvars::rent::Rent::get()?.minimum_balance(Pool::SIZE),
        space: Pool::SIZE as u64,
        owner: program_id,
    }
    .invoke_signed(&[Signer::from(&pool_seeds)])?;

    // Initialize pool state with empty piecewise book sides
    // Oracle will populate prices and quantities via UpdateOracle instruction
    let pool = Pool {
        discriminator: POOL_DISCRIMINATOR,
        bump: ix_data.bump,
        authority: *authority.key(),
        base_mint: *base_mint.key(),
        quote_mint: *quote_mint.key(),
        base_vault: [0u8; 32], // Will be set up separately
        quote_vault: [0u8; 32],
        bid_side: PiecewiseBookSide {
            prices: [0u64; 7],     // Oracle will set
            total_quantity: 0,     // Oracle will set
            consumed: 0,
        },
        ask_side: PiecewiseBookSide {
            prices: [0u64; 7],     // Oracle will set
            total_quantity: 0,     // Oracle will set
            consumed: 0,
        },
        is_active: false, // Start inactive until oracle configures
        _padding: [0; 7],
    };

    // Write pool data
    let pool_bytes = pool.to_bytes();
    let mut pool_data = pool_account.try_borrow_mut_data()?;
    pool_data[..Pool::SIZE].copy_from_slice(&pool_bytes);

    Ok(())
}
