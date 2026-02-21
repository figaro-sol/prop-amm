use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};

use c_u_soon::{Envelope, AUX_DATA_SIZE};
use c_u_soon_cpi::UpdateAuxiliaryDelegated;

use crate::{
    error::PropAmmError,
    math::{buy_base_piecewise, sell_base_piecewise},
    pda::POOL_SEED,
    state::{PropAmmAux, PropAmmAuxProgram, PropAmmQuote},
    token::{get_mint_decimals, transfer_tokens},
};

/// Swap direction
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwapDirection {
    BuyBaseWithQuote = 0,
    SellBaseForQuote = 1,
}

impl SwapDirection {
    pub fn try_from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SwapDirection::BuyBaseWithQuote),
            1 => Some(SwapDirection::SellBaseForQuote),
            _ => None,
        }
    }
}

/// Swap instruction data layout
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwapData {
    pub direction: SwapDirection,
    pub amount_in: u64,
    pub min_amount_out: u64,
}

impl SwapData {
    pub const SIZE: usize = 1 + 8 + 8; // 17 bytes

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        let direction = SwapDirection::try_from_u8(data[0])?;
        let amount_in = u64::from_le_bytes(data[1..9].try_into().ok()?);
        let min_amount_out = u64::from_le_bytes(data[9..17].try_into().ok()?);
        Some(Self {
            direction,
            amount_in,
            min_amount_out,
        })
    }
}

/// Process Swap instruction
///
/// Accounts:
///  0. [signer]    user
///  1. [writable]  envelope            — c_u_soon envelope
///  2. [writable]  user_base_account
///  3. [writable]  user_quote_account
///  4. [writable]  base_vault
///  5. [writable]  quote_vault
///  6. []          base_mint
///  7. []          quote_mint
///  8. []          base_token_program
///  9. []          quote_token_program
/// 10. []          c_u_soon_program
/// 11. []          pool_authority_pda   — delegation_authority, for PDA signing
/// 12. []          padding              — for c_u_soon CPI
pub fn process_swap(_program_id: &Address, accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let ix_data = SwapData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

    if accounts.len() < 13 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let user = &accounts[0];
    let envelope_account = &accounts[1];
    let user_base_account = &accounts[2];
    let user_quote_account = &accounts[3];
    let base_vault = &accounts[4];
    let quote_vault = &accounts[5];
    let base_mint = &accounts[6];
    let quote_mint = &accounts[7];
    let base_token_program = &accounts[8];
    let quote_token_program = &accounts[9];
    let c_u_soon_program = &accounts[10];
    let pool_authority_pda = &accounts[11];
    let padding = &accounts[12];

    if !user.is_signer() {
        return Err(PropAmmError::NotSigner.into());
    }
    if !envelope_account.is_writable()
        || !user_base_account.is_writable()
        || !user_quote_account.is_writable()
        || !base_vault.is_writable()
        || !quote_vault.is_writable()
    {
        return Err(PropAmmError::NotWritable.into());
    }
    if ix_data.amount_in == 0 {
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

    // Read oracle prices
    let quote: &PropAmmQuote = envelope
        .oracle::<PropAmmQuote>()
        .ok_or(PropAmmError::InvalidEnvelope)?;

    // Read aux state
    let aux: &PropAmmAux = envelope
        .aux::<PropAmmAux>()
        .ok_or(PropAmmError::InvalidEnvelope)?;

    // Validate pool is active
    if aux.is_active == 0 {
        return Err(PropAmmError::PoolNotActive.into());
    }

    // Validate mints
    if base_mint.address().as_ref() != &aux.base_mint {
        return Err(PropAmmError::InvalidMint.into());
    }
    if quote_mint.address().as_ref() != &aux.quote_mint {
        return Err(PropAmmError::InvalidMint.into());
    }

    // Validate vaults
    if base_vault.address().as_ref() != &aux.base_vault {
        return Err(PropAmmError::InvalidTokenAccount.into());
    }
    if quote_vault.address().as_ref() != &aux.quote_vault {
        return Err(PropAmmError::InvalidTokenAccount.into());
    }

    // Validate pool_authority_pda is the delegation_authority
    if pool_authority_pda.address() != &envelope.delegation_authority {
        return Err(PropAmmError::InvalidPda.into());
    }

    // Copy mutable state
    let mut updated_aux: PropAmmAux = *aux;
    let oracle_sequence = envelope.oracle_state.sequence;
    let program_aux_sequence = envelope.program_aux_sequence;

    // Get decimals before dropping borrow
    let base_decimals = get_mint_decimals(base_mint)?;
    let quote_decimals = get_mint_decimals(quote_mint)?;

    // Execute swap through typed wrapper (program-only field access)
    let (transfer_in_amount, transfer_out_amount) = {
        let mut aux_w = PropAmmAuxProgram::from_mut(&mut updated_aux);

        // Oracle freshness: reset credit/accumulated on new oracle sequence
        if oracle_sequence > aux_w.accumulated_at_seq {
            *aux_w.bid_credit_mut() = 0;
            *aux_w.bid_accumulated_mut() = 0;
            *aux_w.ask_credit_mut() = 0;
            *aux_w.ask_accumulated_mut() = 0;
            *aux_w.accumulated_at_seq_mut() = oracle_sequence;
        }

        match ix_data.direction {
            SwapDirection::BuyBaseWithQuote => {
                // User spends quote to buy base (consumes ask side)
                let effective_total = aux_w
                    .ask_total_size
                    .checked_add(aux_w.ask_credit)
                    .ok_or(PropAmmError::MathOverflow)?;

                let (base_bought, quote_used, new_consumed) = buy_base_piecewise(
                    ix_data.amount_in,
                    &quote.ask_prices,
                    effective_total,
                    aux_w.ask_accumulated,
                )
                .ok_or(PropAmmError::MathOverflow)?;

                if base_bought < ix_data.min_amount_out {
                    return Err(PropAmmError::SlippageExceeded.into());
                }

                *aux_w.ask_accumulated_mut() = new_consumed;
                if quote_used > 0 {
                    if aux_w.bid_accumulated >= quote_used {
                        *aux_w.bid_accumulated_mut() -= quote_used;
                    } else {
                        let remainder = quote_used - aux_w.bid_accumulated;
                        *aux_w.bid_accumulated_mut() = 0;
                        *aux_w.bid_credit_mut() = aux_w
                            .bid_credit
                            .checked_add(remainder)
                            .ok_or(PropAmmError::MathOverflow)?;
                    }
                }

                (quote_used, base_bought)
            }
            SwapDirection::SellBaseForQuote => {
                // User sells base for quote (consumes bid side)
                let effective_total = aux_w
                    .bid_total_size
                    .checked_add(aux_w.bid_credit)
                    .ok_or(PropAmmError::MathOverflow)?;

                let (quote_received, base_used, new_consumed) = sell_base_piecewise(
                    ix_data.amount_in,
                    &quote.bid_prices,
                    effective_total,
                    aux_w.bid_accumulated,
                )
                .ok_or(PropAmmError::MathOverflow)?;

                if quote_received < ix_data.min_amount_out {
                    return Err(PropAmmError::SlippageExceeded.into());
                }

                *aux_w.bid_accumulated_mut() = new_consumed;
                if base_used > 0 {
                    if aux_w.ask_accumulated >= base_used {
                        *aux_w.ask_accumulated_mut() -= base_used;
                    } else {
                        let remainder = base_used - aux_w.ask_accumulated;
                        *aux_w.ask_accumulated_mut() = 0;
                        *aux_w.ask_credit_mut() = aux_w
                            .ask_credit
                            .checked_add(remainder)
                            .ok_or(PropAmmError::MathOverflow)?;
                    }
                }

                (base_used, quote_received)
            }
        }
    };

    // Drop envelope borrow before CPI
    drop(envelope_data);

    // Token transfers
    let bump_seed = [updated_aux.pool_authority_bump];
    let pool_signer_seeds = [
        Seed::from(POOL_SEED),
        Seed::from(&updated_aux.base_mint),
        Seed::from(&updated_aux.quote_mint),
        Seed::from(&bump_seed),
    ];
    let signer = Signer::from(&pool_signer_seeds);

    match ix_data.direction {
        SwapDirection::BuyBaseWithQuote => {
            // User sends quote to vault (user signs)
            transfer_tokens(
                user_quote_account,
                quote_vault,
                user,
                quote_mint,
                quote_token_program,
                transfer_in_amount,
                quote_decimals,
                &[],
            )?;
            // Vault sends base to user (pool_authority_pda signs)
            transfer_tokens(
                base_vault,
                user_base_account,
                pool_authority_pda,
                base_mint,
                base_token_program,
                transfer_out_amount,
                base_decimals,
                &[signer],
            )?;
        }
        SwapDirection::SellBaseForQuote => {
            // User sends base to vault (user signs)
            transfer_tokens(
                user_base_account,
                base_vault,
                user,
                base_mint,
                base_token_program,
                transfer_in_amount,
                base_decimals,
                &[],
            )?;
            // Vault sends quote to user (pool_authority_pda signs)
            transfer_tokens(
                quote_vault,
                user_quote_account,
                pool_authority_pda,
                quote_mint,
                quote_token_program,
                transfer_out_amount,
                quote_decimals,
                &[signer],
            )?;
        }
    }

    // CPI to c_u_soon: UpdateAuxiliaryDelegated
    let cpi_sequence = program_aux_sequence
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

    UpdateAuxiliaryDelegated {
        envelope: envelope_account,
        delegation_auth: pool_authority_pda,
        padding,
        program: c_u_soon_program,
        sequence: cpi_sequence,
        data: &cpi_aux_data,
    }
    .invoke_signed(&[cpi_signer])
}
