use bytemuck::{Pod, Zeroable};
use c_u_later::CuLater;
use c_u_soon::TypeHash;

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
/// 200 bytes (193 essential + 7 alignment padding).
#[derive(TypeHash, CuLater, Pod, Zeroable, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct PropAmmAux {
    // No marks: settable only via ForceUpdate (both signers)
    pub base_mint: [u8; 32],      // 0..32
    pub quote_mint: [u8; 32],     // 32..64
    pub base_vault: [u8; 32],     // 64..96
    pub quote_vault: [u8; 32],    // 96..128

    // #[authority]: liquidity management
    #[authority]
    pub bid_total_size: u64,      // 128..136
    #[authority]
    pub ask_total_size: u64,      // 136..144
    #[authority]
    pub is_active: u8,            // 144
    #[authority]
    pub _pad_active: [u8; 7],     // 145..152

    // #[program]: swap-mutated state
    #[program]
    pub bid_credit: u64,          // 152..160
    #[program]
    pub bid_accumulated: u64,     // 160..168
    #[program]
    pub ask_credit: u64,          // 168..176
    #[program]
    pub ask_accumulated: u64,     // 176..184
    #[program]
    pub accumulated_at_seq: u64,  // 184..192

    // No mark: immutable after ForceUpdate
    pub pool_authority_bump: u8,  // 192
    pub _pad_bump: [u8; 7],       // 193..200
}

const _: () = assert!(core::mem::size_of::<PropAmmQuote>() == 112);
const _: () = assert!(core::mem::size_of::<PropAmmAux>() == 200);
const _: () = assert!(core::mem::size_of::<PropAmmQuote>() <= 239);
const _: () = assert!(core::mem::size_of::<PropAmmAux>() <= 255);

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
        assert_eq!(core::mem::size_of::<PropAmmAux>(), 200);
    }

    #[test]
    fn test_program_mask() {
        let mask = to_program_wire_mask::<PropAmmAux>();
        // bid_credit (152..160), bid_accumulated (160..168),
        // ask_credit (168..176), ask_accumulated (176..184),
        // accumulated_at_seq (184..192) should be writable (0x00)
        for i in 152..192 {
            assert!(mask.is_writable(i), "byte {} should be program-writable", i);
        }
        // Immutable fields should be blocked
        for i in 0..128 {
            assert!(!mask.is_writable(i), "byte {} should be blocked for program", i);
        }
        // Authority fields should be blocked for program
        for i in 128..152 {
            assert!(!mask.is_writable(i), "byte {} should be blocked for program", i);
        }
        // pool_authority_bump + padding should be blocked
        for i in 192..200 {
            assert!(!mask.is_writable(i), "byte {} should be blocked for program", i);
        }
    }

    #[test]
    fn test_authority_mask() {
        let mask = to_authority_wire_mask::<PropAmmAux>();
        // bid_total_size (128..136), ask_total_size (136..144),
        // is_active (144), _pad_active (145..152) should be writable
        for i in 128..152 {
            assert!(mask.is_writable(i), "byte {} should be authority-writable", i);
        }
        // Immutable fields should be blocked
        for i in 0..128 {
            assert!(!mask.is_writable(i), "byte {} should be blocked for authority", i);
        }
        // Program fields should be blocked for authority
        for i in 152..192 {
            assert!(!mask.is_writable(i), "byte {} should be blocked for authority", i);
        }
    }
}
