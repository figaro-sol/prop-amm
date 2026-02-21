use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};

use c_u_soon::{Envelope, AUX_DATA_SIZE};
use c_u_soon_cpi::UpdateAuxiliary;

use crate::{
    error::PropAmmError,
    pda::POOL_SEED,
    state::{PropAmmAux, PropAmmAuxAuthority},
    token::{get_mint_decimals, transfer_tokens},
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
///  1. [writable]  envelope               — c_u_soon envelope
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
    if !envelope_account.is_writable()
        || !authority_token_account.is_writable()
        || !vault.is_writable()
    {
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

    // Validate mint, vault, and withdrawable amount
    match ix_data.side {
        TokenSide::Base => {
            if mint.address().as_ref() != &aux.base_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if vault.address().as_ref() != &aux.base_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
            let effective = aux
                .ask_total_size
                .checked_add(aux.ask_credit)
                .ok_or(PropAmmError::MathOverflow)?;
            let withdrawable = effective
                .checked_sub(aux.ask_accumulated)
                .ok_or(PropAmmError::MathOverflow)?;
            if withdrawable < ix_data.amount {
                return Err(PropAmmError::InsufficientFunds.into());
            }
        }
        TokenSide::Quote => {
            if mint.address().as_ref() != &aux.quote_mint {
                return Err(PropAmmError::InvalidMint.into());
            }
            if vault.address().as_ref() != &aux.quote_vault {
                return Err(PropAmmError::InvalidTokenAccount.into());
            }
            let effective = aux
                .bid_total_size
                .checked_add(aux.bid_credit)
                .ok_or(PropAmmError::MathOverflow)?;
            let withdrawable = effective
                .checked_sub(aux.bid_accumulated)
                .ok_or(PropAmmError::MathOverflow)?;
            if withdrawable < ix_data.amount {
                return Err(PropAmmError::InsufficientFunds.into());
            }
        }
    }

    // Copy mutable state and read sequence before dropping borrow
    let mut updated_aux: PropAmmAux = *aux;
    let authority_aux_sequence = envelope.authority_aux_sequence;

    // Get decimals
    let decimals = get_mint_decimals(mint)?;

    // Drop borrow before CPI
    drop(envelope_data);

    // Update total_size through typed wrapper (authority-only field access)
    {
        let mut aux_w = PropAmmAuxAuthority::from_mut(&mut updated_aux);
        match ix_data.side {
            TokenSide::Base => {
                *aux_w.ask_total_size_mut() = aux_w
                    .ask_total_size
                    .checked_sub(ix_data.amount)
                    .ok_or(PropAmmError::MathOverflow)?;
            }
            TokenSide::Quote => {
                *aux_w.bid_total_size_mut() = aux_w
                    .bid_total_size
                    .checked_sub(ix_data.amount)
                    .ok_or(PropAmmError::MathOverflow)?;
            }
        }
    }

    // Token transfer: vault → authority (pool_authority_pda signs)
    let bump_seed = [updated_aux.pool_authority_bump];
    let pool_signer_seeds = [
        Seed::from(POOL_SEED),
        Seed::from(&updated_aux.base_mint),
        Seed::from(&updated_aux.quote_mint),
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
    )?;

    // CPI to c_u_soon: UpdateAuxiliary (authority path)
    let cpi_sequence = authority_aux_sequence
        .checked_add(1)
        .ok_or(PropAmmError::MathOverflow)?;

    let mut cpi_aux_data = [0u8; AUX_DATA_SIZE];
    let aux_bytes = bytemuck::bytes_of(&updated_aux);
    cpi_aux_data[..aux_bytes.len()].copy_from_slice(aux_bytes);

    let pool_signer_seeds2 = [
        Seed::from(POOL_SEED),
        Seed::from(&updated_aux.base_mint),
        Seed::from(&updated_aux.quote_mint),
        Seed::from(&bump_seed),
    ];
    let cpi_signer = Signer::from(&pool_signer_seeds2);

    UpdateAuxiliary {
        authority,
        envelope: envelope_account,
        pda: pool_authority_pda,
        program: c_u_soon_program,
        sequence: cpi_sequence,
        data: &cpi_aux_data,
    }
    .invoke_signed(&[cpi_signer])
}
