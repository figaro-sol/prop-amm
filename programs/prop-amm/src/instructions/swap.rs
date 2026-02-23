use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};

use c_u_soon::TypeHash;
use c_u_soon_cpi::{next_sequence, UpdateAuxiliaryDelegatedMultiRange};

use crate::{
    error::PropAmmError,
    math::{buy_base_piecewise, sell_base_piecewise},
    pda::POOL_SEED,
    state::{
        load_quote_aux, load_validated_envelope, PropAmmAux, PropAmmAuxProgram,
        PropAmmAuxProgramDelta,
    },
    token::{get_mint_decimals, get_token_account_balance, transfer_tokens},
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
pub fn process_swap(_program_id: &Address, accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let ix_data = SwapData::from_bytes(data).ok_or(ProgramError::InvalidInstructionData)?;

    if accounts.len() < 12 {
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

    let envelope = load_validated_envelope(envelope_account, c_u_soon_program)?;
    let (quote, aux) = load_quote_aux(&envelope)?;

    // Validate pool is active
    if aux.is_active == 0 {
        return Err(PropAmmError::PoolNotActive.into());
    }

    // Validate mints
    if base_mint.address().as_ref() != aux.base_mint {
        return Err(PropAmmError::InvalidMint.into());
    }
    if quote_mint.address().as_ref() != aux.quote_mint {
        return Err(PropAmmError::InvalidMint.into());
    }

    // Validate vaults
    if base_vault.address().as_ref() != aux.base_vault {
        return Err(PropAmmError::InvalidTokenAccount.into());
    }
    if quote_vault.address().as_ref() != aux.quote_vault {
        return Err(PropAmmError::InvalidTokenAccount.into());
    }

    // Validate pool_authority_pda is the delegation_authority
    if pool_authority_pda.address() != &envelope.delegation_authority {
        return Err(PropAmmError::InvalidPda.into());
    }

    // Snapshot immutable fields for PDA signing and mutable state for swap math
    let base_mint_bytes = aux.base_mint;
    let quote_mint_bytes = aux.quote_mint;
    let pool_authority_bump = aux.pool_authority_bump;
    let mut updated_aux: PropAmmAux = *aux;
    let oracle_sequence = envelope.oracle_state.sequence;
    let program_aux_sequence = envelope.program_aux_sequence;

    // Get decimals and vault balances before dropping borrow
    let base_decimals = get_mint_decimals(base_mint)?;
    let quote_decimals = get_mint_decimals(quote_mint)?;
    let base_vault_balance = get_token_account_balance(base_vault)?;
    let quote_vault_balance = get_token_account_balance(quote_vault)?;

    // Execute swap through typed wrapper (program-only field access)
    let (transfer_in_amount, transfer_out_amount) = {
        let mut aux_w = PropAmmAuxProgram::from_mut(&mut updated_aux);

        // Oracle freshness: reset accumulated on new oracle sequence.
        if oracle_sequence > aux_w.accumulated_at_seq {
            *aux_w.bid_accumulated_mut() = 0;
            *aux_w.ask_accumulated_mut() = 0;
            *aux_w.accumulated_at_seq_mut() = oracle_sequence;
        }

        match ix_data.direction {
            SwapDirection::BuyBaseWithQuote => {
                // User spends quote to buy base (consumes ask side).
                // Vault balance is ground truth: effective = base_vault + ask_accumulated.
                let effective_total = base_vault_balance
                    .checked_add(aux_w.ask_accumulated)
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
                *aux_w.bid_accumulated_mut() = aux_w.bid_accumulated.saturating_sub(quote_used);

                (quote_used, base_bought)
            }
            SwapDirection::SellBaseForQuote => {
                // User sells base for quote (consumes bid side).
                // Vault balance is ground truth: effective = quote_vault + bid_accumulated.
                let effective_total = quote_vault_balance
                    .checked_add(aux_w.bid_accumulated)
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
                *aux_w.ask_accumulated_mut() = aux_w.ask_accumulated.saturating_sub(base_used);

                (base_used, quote_received)
            }
        }
    };

    // Build delta with only the changed #[program] fields
    let mut delta = PropAmmAuxProgramDelta::new();
    delta
        .set_bid_accumulated(updated_aux.bid_accumulated)
        .set_ask_accumulated(updated_aux.ask_accumulated)
        .set_accumulated_at_seq(updated_aux.accumulated_at_seq);
    let write_specs = delta.to_write_specs();

    // Drop envelope borrow before CPI
    drop(envelope);

    // Token transfers
    let bump_seed = [pool_authority_bump];
    let pool_signer_seeds = [
        Seed::from(POOL_SEED),
        Seed::from(&base_mint_bytes),
        Seed::from(&quote_mint_bytes),
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

    // CPI to c_u_soon: UpdateAuxiliaryDelegatedMultiRange
    let cpi_sequence = next_sequence(program_aux_sequence)?;

    let pool_signer_seeds2 = [
        Seed::from(POOL_SEED),
        Seed::from(&base_mint_bytes),
        Seed::from(&quote_mint_bytes),
        Seed::from(&bump_seed),
    ];
    let cpi_signer = Signer::from(&pool_signer_seeds2);

    UpdateAuxiliaryDelegatedMultiRange {
        envelope: envelope_account,
        delegation_auth: pool_authority_pda,
        padding: user,
        program: c_u_soon_program,
        metadata: PropAmmAux::METADATA.as_u64(),
        sequence: cpi_sequence,
        ranges: &write_specs,
    }
    .invoke_signed(&[cpi_signer])
}
