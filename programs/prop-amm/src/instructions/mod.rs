pub mod swap;
pub mod withdraw;

/// Instruction discriminators (single byte)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instruction {
    /// Execute a swap
    Swap = 0,

    /// Withdraw tokens from pool (authority only)
    Withdraw = 1,
}

impl Instruction {
    pub fn try_from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Instruction::Swap),
            1 => Some(Instruction::Withdraw),
            _ => None,
        }
    }
}

/// Token side for withdraw
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenSide {
    /// Base token - affects ask_side
    Base = 0,
    /// Quote token - affects bid_side
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
