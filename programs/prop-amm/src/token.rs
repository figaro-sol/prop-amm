//! Token Transfer Helper
//!
//! Supports both SPL Token and Token-2022 programs.
//! Detects the correct program from mint account owner.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction, Signer},
    program::invoke_signed,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::error::PropAmmError;

/// SPL Token Program ID
pub const SPL_TOKEN_PROGRAM_ID: Pubkey = [
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93,
    0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91,
    0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
];

/// Token-2022 Program ID
pub const TOKEN_2022_PROGRAM_ID: Pubkey = [
    0x06, 0xdd, 0xf6, 0xe1, 0xee, 0x75, 0x8f, 0xde,
    0x18, 0x42, 0x5d, 0xbc, 0xe4, 0x6c, 0xcd, 0xda,
    0xb6, 0x1a, 0xfc, 0x4d, 0x83, 0xb9, 0x0d, 0x27,
    0xfe, 0xbd, 0xf9, 0x28, 0xd8, 0xa1, 0x8b, 0xfc,
];

/// TransferChecked instruction discriminator (works for both programs)
const TRANSFER_CHECKED_IX: u8 = 12;

/// Transfer tokens using the appropriate token program.
///
/// Detects whether to use SPL Token or Token-2022 based on mint owner.
/// Uses transfer_checked which works for both programs and is required for Token-2022.
///
/// # Arguments
/// * `from` - Source token account
/// * `to` - Destination token account
/// * `authority` - Transfer authority (user or PDA)
/// * `mint` - Token mint account (used to detect program and get decimals)
/// * `token_program` - The token program account
/// * `amount` - Amount to transfer (native units)
/// * `decimals` - Token decimals
/// * `signer_seeds` - Optional PDA signer seeds
pub fn transfer_tokens(
    from: &AccountInfo,
    to: &AccountInfo,
    authority: &AccountInfo,
    mint: &AccountInfo,
    token_program: &AccountInfo,
    amount: u64,
    decimals: u8,
    signer_seeds: &[Signer],
) -> ProgramResult {
    // Validate token program
    let program_id = token_program.key();
    if program_id != &SPL_TOKEN_PROGRAM_ID && program_id != &TOKEN_2022_PROGRAM_ID {
        return Err(PropAmmError::InvalidTokenProgram.into());
    }

    // Build transfer_checked instruction data
    // Layout: [12] + amount(8 LE) + decimals(1)
    let mut data = [0u8; 10];
    data[0] = TRANSFER_CHECKED_IX;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = decimals;

    // Account metas for transfer_checked:
    // 0. [writable] source
    // 1. [] mint
    // 2. [writable] destination
    // 3. [signer] authority
    let account_metas = [
        AccountMeta::writable(from.key()),
        AccountMeta::readonly(mint.key()),
        AccountMeta::writable(to.key()),
        AccountMeta::readonly_signer(authority.key()),
    ];

    let instruction = Instruction {
        program_id,
        accounts: &account_metas,
        data: &data,
    };

    // Invoke with or without signer seeds
    // Note: Only pass the 4 accounts referenced by the instruction, not the token_program
    invoke_signed(
        &instruction,
        &[from, mint, to, authority],
        signer_seeds,
    )
}

/// Get decimals from a mint account.
///
/// Mint layout:
/// - 0-36: mint_authority (COption<Pubkey>)
/// - 36-44: supply (u64)
/// - 44: decimals (u8)
pub fn get_mint_decimals(mint: &AccountInfo) -> Result<u8, PropAmmError> {
    let data = mint.try_borrow_data().map_err(|_| PropAmmError::AccountDataTooSmall)?;
    if data.len() < 45 {
        return Err(PropAmmError::AccountDataTooSmall);
    }
    Ok(data[44])
}

/// Check if a mint is Token-2022
pub fn is_token_2022(mint: &AccountInfo) -> bool {
    let owner = unsafe { mint.owner() };
    owner == &TOKEN_2022_PROGRAM_ID
}
