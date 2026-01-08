//! Instruction Handlers
//!
//! Each instruction has its own module with processing logic.

pub mod deposit;
pub mod initialize;
pub mod set_vaults;
pub mod swap;
pub mod swap_event;
pub mod update_oracle;
pub mod withdraw;

/// Instruction discriminators (single byte)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instruction {
    /// Initialize a new pool
    Initialize = 0,

    /// Update oracle (fair value and curve parameters)
    UpdateOracle = 1,

    /// Execute a swap
    Swap = 2,

    /// Set vault addresses (one-time setup after initialize)
    SetVaults = 3,

    /// Deposit tokens into pool (authority only)
    Deposit = 4,

    /// Withdraw tokens from pool (authority only)
    Withdraw = 5,

    /// Swap event (no-op, emitted via self-CPI for trade logging)
    SwapEvent = 6,
}

impl Instruction {
    /// Try to parse instruction from discriminator byte
    pub fn try_from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Instruction::Initialize),
            1 => Some(Instruction::UpdateOracle),
            2 => Some(Instruction::Swap),
            3 => Some(Instruction::SetVaults),
            4 => Some(Instruction::Deposit),
            5 => Some(Instruction::Withdraw),
            6 => Some(Instruction::SwapEvent),
            _ => None,
        }
    }
}
