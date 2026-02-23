use bytemuck::{Pod, Zeroable};
use c_u_later::CuLater;
use c_u_soon::{Envelope, TypeHash};
use pinocchio::{account::Ref, error::ProgramError, AccountView};

use crate::error::PropAmmError;

pub const SCALE: u64 = 1_000_000_000;
pub const NUM_PRICE_POINTS: usize = 7;
pub const NUM_SEGMENTS: usize = 6;

/// Fast-path oracle data: bid/ask prices written by MM authority.
/// 112 bytes, fits within 239-byte oracle region.
#[derive(TypeHash, Pod, Zeroable, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct PropAmmQuote {
    pub bid_prices: [u64; NUM_PRICE_POINTS], // 56B
    pub ask_prices: [u64; NUM_PRICE_POINTS], // 56B
}

/// Auxiliary envelope data: all mutable pool state.
/// 168 bytes.
#[derive(TypeHash, CuLater, Pod, Zeroable, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct PropAmmAux {
    // No marks: settable only via ForceUpdate (both signers)
    pub base_mint: [u8; 32],   // 0..32
    pub quote_mint: [u8; 32],  // 32..64
    pub base_vault: [u8; 32],  // 64..96
    pub quote_vault: [u8; 32], // 96..128

    // #[authority]: liquidity management
    #[authority]
    pub is_active: u8, // 128
    #[authority]
    pub _pad_active: [u8; 7], // 129..136

    // #[program]: swap-mutated state
    #[program]
    pub bid_accumulated: u64, // 136..144
    #[program]
    pub ask_accumulated: u64, // 144..152
    #[program]
    pub accumulated_at_seq: u64, // 152..160

    // No mark: immutable after ForceUpdate
    pub pool_authority_bump: u8, // 160
    pub _pad_bump: [u8; 7],      // 161..168
}

const _: () = assert!(core::mem::size_of::<PropAmmQuote>() == 112);
const _: () = assert!(core::mem::size_of::<PropAmmAux>() == 168);
const _: () = assert!(core::mem::size_of::<PropAmmQuote>() <= 239);

/// Local computation struct for swap math.
/// Not stored on-chain — constructed from aux fields + oracle prices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PiecewiseBookSide {
    pub prices: [u64; NUM_PRICE_POINTS],
    pub total_quantity: u64,
    pub consumed: u64,
}

impl PiecewiseBookSide {
    #[inline]
    pub fn remaining(&self) -> u64 {
        self.total_quantity.saturating_sub(self.consumed)
    }

    #[inline]
    pub fn quantity_per_segment(&self) -> u64 {
        self.total_quantity / NUM_SEGMENTS as u64
    }
}

pub fn load_validated_envelope<'a>(
    envelope_account: &'a AccountView,
    c_u_soon_program: &AccountView,
) -> Result<Ref<'a, Envelope>, ProgramError> {
    // Envelope must be owned by c_u_soon program
    if !envelope_account.owned_by(c_u_soon_program.address()) {
        return Err(PropAmmError::InvalidOwner.into());
    }

    Ref::try_map(envelope_account.try_borrow()?, bytemuck::try_from_bytes)
        .map_err(|_| PropAmmError::InvalidEnvelope.into())
}
pub fn load_quote_aux(envelope: &Envelope) -> Result<(&PropAmmQuote, &PropAmmAux), ProgramError> {
    // Read oracle prices
    let quote = envelope
        .oracle::<PropAmmQuote>()
        .ok_or(PropAmmError::InvalidEnvelope)?;

    // Read aux state
    let aux: &PropAmmAux = envelope
        .aux::<PropAmmAux>()
        .ok_or(PropAmmError::InvalidEnvelope)?;
    Ok((quote, aux))
}

#[cfg(test)]
mod tests {
    use super::*;
    use c_u_later::{to_authority_wire_mask, to_program_wire_mask};

    #[test]
    fn test_quote_size() {
        assert_eq!(core::mem::size_of::<PropAmmQuote>(), 112);
    }

    #[test]
    fn test_aux_size() {
        assert_eq!(core::mem::size_of::<PropAmmAux>(), 168);
    }

    #[test]
    fn test_program_mask() {
        let mask = to_program_wire_mask::<PropAmmAux>();
        // bid_accumulated (136..144), ask_accumulated (144..152),
        // accumulated_at_seq (152..160) should be writable
        for i in 136..160 {
            assert!(mask.is_writable(i), "byte {} should be program-writable", i);
        }
        // Immutable fields should be blocked
        for i in 0..128 {
            assert!(
                !mask.is_writable(i),
                "byte {} should be blocked for program",
                i
            );
        }
        // Authority fields should be blocked for program
        for i in 128..136 {
            assert!(
                !mask.is_writable(i),
                "byte {} should be blocked for program",
                i
            );
        }
        // pool_authority_bump + padding should be blocked
        for i in 160..168 {
            assert!(
                !mask.is_writable(i),
                "byte {} should be blocked for program",
                i
            );
        }
    }

    #[test]
    fn test_authority_mask() {
        let mask = to_authority_wire_mask::<PropAmmAux>();
        // is_active (128), _pad_active (129..136) should be writable
        for i in 128..136 {
            assert!(
                mask.is_writable(i),
                "byte {} should be authority-writable",
                i
            );
        }
        // Immutable fields should be blocked
        for i in 0..128 {
            assert!(
                !mask.is_writable(i),
                "byte {} should be blocked for authority",
                i
            );
        }
        // Program fields should be blocked for authority
        for i in 136..160 {
            assert!(
                !mask.is_writable(i),
                "byte {} should be blocked for authority",
                i
            );
        }
    }
}
