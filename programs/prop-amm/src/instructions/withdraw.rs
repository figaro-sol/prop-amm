use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};

use c_u_soon::Envelope;

use crate::{
    error::PropAmmError,
    pda::POOL_SEED,
    state::PropAmmAux,
    token::{get_mint_decimals, get_token_account_balance, transfer_tokens},
};

use super::TokenSide;

/// Withdraw instruction data layout
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WithdrawData {
    pub side: TokenSide,
    pub amount: u64,
}

impl WithdrawData {
    pub const SIZE: usize = 1 + 8; // 9 bytes

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
///  0. [signer]    authority              — envelope authority
///  1. []          envelope               — c_u_soon envelope (readonly)
///  2. [writable]  authority_token_account — destination
///  3. [writable]  vault                  — pool's token vault (source)
///  4. []          mint
///  5. []          token_program
///  6. []          c_u_soon_program
///  7. []          pool_authority_pda     — for PDA signing
pub fn process_withdraw(
    _program_id: &Address,
    accounts: &[AccountView],
    data: &[u8],
) -> ProgramResult {
    let ix_data = WithdrawData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

    if accounts.len() < 8 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let envelope_account = &accounts[1];
    let authority_token_account = &accounts[2];
    let vault = &accounts[3];
    let mint = &accounts[4];
    let token_program = &accounts[5];
    let c_u_soon_program = &accounts[6];
    let pool_authority_pda = &accounts[7];

    if !authority.is_signer() {
        return Err(PropAmmError::NotSigner.into());
    }
    if !authority_token_account.is_writable() || !vault.is_writable() {
        return Err(PropAmmError::NotWritable.into());
    }
    if ix_data.amount == 0 {
        return Err(PropAmmError::ZeroAmount.into());
    }

    // Envelope must be owned by c_u_soon program
    if !envelope_account.owned_by(c_u_soon_program.address()) {
        return Err(PropAmmError::InvalidOwner.into());
    }

    // Read envelope
    let envelope_data = envelope_account.try_borrow()?;
    if envelope_data.len() < Envelope::SIZE {
        return Err(PropAmmError::InvalidEnvelope.into());
    }
    let envelope: &Envelope = bytemuck::from_bytes(&envelope_data[..Envelope::SIZE]);

    // Verify authority
    if authority.address() != &envelope.authority {
        return Err(PropAmmError::Unauthorized.into());
    }

    // Read aux state
    let aux: &PropAmmAux = envelope
        .aux::<PropAmmAux>()
        .ok_or(PropAmmError::InvalidEnvelope)?;

    // Validate pool_authority_pda
    if pool_authority_pda.address() != &envelope.delegation_authority {
        return Err(PropAmmError::InvalidPda.into());
    }

    // Validate mint and vault identity
    match ix_data.side {
        TokenSide::Base => {
            if mint.address().as_ref() != &aux.base_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if vault.address().as_ref() != &aux.base_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
        }
        TokenSide::Quote => {
            if mint.address().as_ref() != &aux.quote_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if vault.address().as_ref() != &aux.quote_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
        }
    }

    let pool_authority_bump = aux.pool_authority_bump;
    let base_mint = aux.base_mint;
    let quote_mint = aux.quote_mint;

    let decimals = get_mint_decimals(mint)?;

    drop(envelope_data);

    // Vault balance is ground truth: withdraw only what is actually there
    let vault_balance = get_token_account_balance(vault)?;
    if vault_balance < ix_data.amount {
        return Err(PropAmmError::InsufficientFunds.into());
    }

    // Token transfer: vault → authority (pool_authority_pda signs)
    let bump_seed = [pool_authority_bump];
    let pool_signer_seeds = [
        Seed::from(POOL_SEED),
        Seed::from(&base_mint),
        Seed::from(&quote_mint),
        Seed::from(&bump_seed),
    ];
    let signer = Signer::from(&pool_signer_seeds);

    transfer_tokens(
        vault,
        authority_token_account,
        pool_authority_pda,
        mint,
        token_program,
        ix_data.amount,
        decimals,
        &[signer],
    )
}
