use pinocchio::error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropAmmError {
    InvalidInstruction = 0,
    InvalidOwner = 1,
    InvalidPda = 2,
    NotSigner = 3,
    NotWritable = 4,
    InsufficientLiquidity = 5,
    SlippageExceeded = 6,
    MathOverflow = 7,
    InvalidPriceRange = 8,
    PoolNotActive = 9,
    Unauthorized = 10,
    InvalidTokenAccount = 11,
    ZeroAmount = 12,
    InvalidDiscriminator = 13,
    AccountDataTooSmall = 14,
    InvalidMint = 15,
    InvalidTokenProgram = 16,
    InvalidPrices = 17,
    InsufficientFunds = 18,
    InvalidEnvelope = 19,
    OracleStale = 20,
}

impl From<PropAmmError> for ProgramError {
    fn from(e: PropAmmError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl PropAmmError {
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
            PropAmmError::InvalidEnvelope => "Invalid c_u_soon envelope",
            PropAmmError::OracleStale => "Oracle data is stale",
        }
    }
}
