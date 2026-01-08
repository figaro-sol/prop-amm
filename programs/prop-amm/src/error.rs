//! Custom Program Errors
//!
//! Defines error types for the Prop AMM program.

use pinocchio::program_error::ProgramError;

/// Custom error codes for PropAmm program
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropAmmError {
    /// Invalid instruction discriminator
    InvalidInstruction = 0,

    /// Invalid account owner
    InvalidOwner = 1,

    /// Invalid PDA derivation
    InvalidPda = 2,

    /// Required signer is missing
    NotSigner = 3,

    /// Account must be writable
    NotWritable = 4,

    /// Insufficient liquidity for the requested swap
    InsufficientLiquidity = 5,

    /// Output amount is less than minimum specified (slippage exceeded)
    SlippageExceeded = 6,

    /// Math operation overflow
    MathOverflow = 7,

    /// Invalid price range (lower must be less than upper)
    InvalidPriceRange = 8,

    /// Pool is not active for trading
    PoolNotActive = 9,

    /// Signer is not the pool authority
    Unauthorized = 10,

    /// Invalid token account
    InvalidTokenAccount = 11,

    /// Zero amount is not allowed
    ZeroAmount = 12,

    /// Invalid discriminator on account
    InvalidDiscriminator = 13,

    /// Account data too small
    AccountDataTooSmall = 14,

    /// Invalid mint for this pool
    InvalidMint = 15,

    /// Invalid token program
    InvalidTokenProgram = 16,

    /// Invalid prices (not strictly ascending)
    InvalidPrices = 17,

    /// Insufficient funds to withdraw (consumed exceeds available)
    InsufficientFunds = 18,
}

impl From<PropAmmError> for ProgramError {
    fn from(e: PropAmmError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl PropAmmError {
    /// Convert error code to human-readable message
    pub fn message(&self) -> &'static str {
        match self {
            PropAmmError::InvalidInstruction => "Invalid instruction",
            PropAmmError::InvalidOwner => "Invalid account owner",
            PropAmmError::InvalidPda => "Invalid PDA derivation",
            PropAmmError::NotSigner => "Required signer is missing",
            PropAmmError::NotWritable => "Account must be writable",
            PropAmmError::InsufficientLiquidity => "Insufficient liquidity",
            PropAmmError::SlippageExceeded => "Slippage tolerance exceeded",
            PropAmmError::MathOverflow => "Math overflow",
            PropAmmError::InvalidPriceRange => "Invalid price range",
            PropAmmError::PoolNotActive => "Pool is not active",
            PropAmmError::Unauthorized => "Unauthorized",
            PropAmmError::InvalidTokenAccount => "Invalid token account",
            PropAmmError::ZeroAmount => "Zero amount not allowed",
            PropAmmError::InvalidDiscriminator => "Invalid account discriminator",
            PropAmmError::AccountDataTooSmall => "Account data too small",
            PropAmmError::InvalidMint => "Invalid mint for this pool",
            PropAmmError::InvalidTokenProgram => "Invalid token program",
            PropAmmError::InvalidPrices => "Prices must be strictly ascending",
            PropAmmError::InsufficientFunds => "Insufficient funds to withdraw",
        }
    }
}
