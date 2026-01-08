import {
  PublicKey,
  TransactionInstruction,
  SystemProgram,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

// Program ID for the Piecewise Prop AMM (mainnet)
export const PROP_AMM_PROGRAM_ID = new PublicKey(
  "13NnzndP9KHoqygdU6Bba1cy4EayKN3F3AX6F3PSALnK"
);

/**
 * ## Price Convention
 *
 * Prices are stored as native-ratio scaled values:
 *   price = (quote_native_units / base_native_units) × PRICE_SCALE
 *
 * This allows the on-chain math to work purely on native token units
 * without any decimal awareness. The client handles human↔native conversion.
 *
 * Example for NVDAX(8 dec)/USDC(6 dec) at $200:
 * - 1 NVDAX token = 10^8 base native units
 * - 200 USDC = 2×10^8 quote native units
 * - Native ratio = 2×10^8 / 10^8 = 2
 * - Stored price = 2 × PRICE_SCALE = 2×10^9
 */

// Price scaling factor: 10^9
export const PRICE_SCALE = 1_000_000_000n;

// USDC has 6 decimals
export const USDC_DECIMALS = 6;
export const USDC_SCALE = 1_000_000n;

// Common token decimals
export const SOL_DECIMALS = 9;
export const SOL_SCALE = 1_000_000_000n;

// Instruction discriminators
const IX_INITIALIZE = 0;
const IX_UPDATE_ORACLE = 1;
const IX_SWAP = 2;
const IX_SET_VAULTS = 3;
const IX_DEPOSIT = 4;
const IX_WITHDRAW = 5;

// Swap directions (matches program)
export const SWAP_BUY_EXACT_IN = 0;   // BuyBaseWithQuote
export const SWAP_SELL_EXACT_OUT = 1; // SellBaseForQuote

// Token sides for deposit/withdraw
export const TOKEN_SIDE_BASE = 0;  // Base token - affects ask_side.total_quantity
export const TOKEN_SIDE_QUOTE = 1; // Quote token - affects bid_side.total_quantity

// Number of price points per side (creates 6 segments)
export const NUM_PRICE_POINTS = 7;
export const NUM_SEGMENTS = 6;

/**
 * Helper to write u64 as little-endian bytes
 */
function writeU64LE(value: bigint): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(value);
  return buf;
}

/**
 * Convert human-readable price to native-ratio price for on-chain storage
 *
 * @param humanPrice - Human-readable price (e.g., 200 for "$200")
 * @param baseDecimals - Base token decimals (e.g., 8 for NVDAX)
 * @param quoteDecimals - Quote token decimals (e.g., 6 for USDC)
 * @returns Native-ratio price scaled by PRICE_SCALE
 *
 * @example
 * // $200 NVDAX/USDC price
 * humanToNativePrice(200, 8, 6) // Returns 2n * PRICE_SCALE = 2_000_000_000n
 */
export function humanToNativePrice(
  humanPrice: number,
  baseDecimals: number,
  quoteDecimals: number
): bigint {
  // nativePrice = humanPrice × quoteScale / baseScale × PRICE_SCALE / PRICE_SCALE
  // Simplified: nativePrice = humanPrice × 10^quoteDecimals / 10^baseDecimals × PRICE_SCALE
  const quoteScale = 10 ** quoteDecimals;
  const baseScale = 10 ** baseDecimals;
  const ratio = humanPrice * quoteScale / baseScale;
  return BigInt(Math.floor(ratio * Number(PRICE_SCALE)));
}

/**
 * Convert native-ratio price back to human-readable price
 *
 * @param nativePrice - Native-ratio price from pool state
 * @param baseDecimals - Base token decimals
 * @param quoteDecimals - Quote token decimals
 * @returns Human-readable price
 *
 * @example
 * // Convert stored price back to $200
 * nativeToHumanPrice(2_000_000_000n, 8, 6) // Returns 200
 */
export function nativeToHumanPrice(
  nativePrice: bigint,
  baseDecimals: number,
  quoteDecimals: number
): number {
  const quoteScale = 10 ** quoteDecimals;
  const baseScale = 10 ** baseDecimals;
  const ratio = Number(nativePrice) / Number(PRICE_SCALE);
  return ratio * baseScale / quoteScale;
}

/**
 * Legacy function - converts dollar amount to scaled price assuming equal decimals
 * @deprecated Use humanToNativePrice with explicit decimals instead
 */
export function dollarToScaledPrice(dollars: number): bigint {
  return BigInt(Math.floor(dollars * Number(PRICE_SCALE)));
}

/**
 * Convert USDC amount to micro-units
 */
export function usdcToMicroUnits(usdc: number): bigint {
  return BigInt(Math.floor(usdc * Number(USDC_SCALE)));
}

/**
 * Convert SOL amount to lamports
 */
export function solToLamports(sol: number): bigint {
  return BigInt(Math.floor(sol * Number(SOL_SCALE)));
}

/**
 * Convert token amount to native units
 */
export function toNativeUnits(amount: number, decimals: number): bigint {
  return BigInt(Math.floor(amount * (10 ** decimals)));
}

export interface InitializeParams {
  bump: number;
  // Note: Prices and quantities are now set via UpdateOracle instruction
}

/**
 * Create Initialize instruction
 *
 * Creates a new pool with empty book sides. Use UpdateOracle to set prices/quantities.
 *
 * Accounts:
 * 0. [signer] Authority
 * 1. [writable] Pool (PDA)
 * 2. [] Base Mint
 * 3. [] Quote Mint
 * 4. [] System Program
 */
export function createInitializeInstruction(
  programId: PublicKey,
  authority: PublicKey,
  pool: PublicKey,
  baseMint: PublicKey,
  quoteMint: PublicKey,
  params: InitializeParams
): TransactionInstruction {
  // Data: discriminator(1) + bump(1) = 2 bytes
  const data = Buffer.concat([
    Buffer.from([IX_INITIALIZE]),
    Buffer.from([params.bump]),
  ]);

  return new TransactionInstruction({
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: baseMint, isSigner: false, isWritable: false },
      { pubkey: quoteMint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data,
  });
}

/**
 * Oracle update parameters - PRICES ONLY
 *
 * Note: Oracle does NOT set quantities. Quantities are determined by on-chain
 * state only (deposits, withdrawals, and swap auto-crediting). This avoids
 * latency race conditions between reading balances off-chain and landing the tx.
 */
export interface UpdateOracleParams {
  bidPrices: bigint[];  // 7 price points for bid side (must be strictly ascending)
  askPrices: bigint[];  // 7 price points for ask side (must be strictly ascending)
}

/**
 * Create UpdateOracle instruction
 *
 * Sets 7 price points for each side. Prices must be strictly ascending.
 * Resets consumed to 0 on both sides (fresh curve position).
 * Does NOT modify total_quantity - that's determined by on-chain state only.
 *
 * Data layout (112 bytes):
 * - bid_prices: [u64; 7] (56 bytes)
 * - ask_prices: [u64; 7] (56 bytes)
 *
 * Accounts:
 * 0. [signer] Authority
 * 1. [writable] Pool
 */
export function createUpdateOracleInstruction(
  programId: PublicKey,
  authority: PublicKey,
  pool: PublicKey,
  params: UpdateOracleParams
): TransactionInstruction {
  if (params.bidPrices.length !== NUM_PRICE_POINTS) {
    throw new Error(`Bid side must have exactly ${NUM_PRICE_POINTS} prices`);
  }
  if (params.askPrices.length !== NUM_PRICE_POINTS) {
    throw new Error(`Ask side must have exactly ${NUM_PRICE_POINTS} prices`);
  }

  // Data layout: discriminator(1) + bid_prices(56) + ask_prices(56) = 113 bytes
  const buffers: Buffer[] = [Buffer.from([IX_UPDATE_ORACLE])];

  // Bid side: 7 prices only
  for (const price of params.bidPrices) {
    buffers.push(writeU64LE(price));
  }

  // Ask side: 7 prices only
  for (const price of params.askPrices) {
    buffers.push(writeU64LE(price));
  }

  const data = Buffer.concat(buffers);

  return new TransactionInstruction({
    keys: [
      { pubkey: authority, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
    ],
    programId,
    data,
  });
}

/**
 * Create SetVaults instruction
 *
 * Accounts:
 * 0. [signer] Authority
 * 1. [writable] Pool
 * 2. [] Base Vault (wSOL token account)
 * 3. [] Quote Vault (USDC token account)
 */
export function createSetVaultsInstruction(
  programId: PublicKey,
  authority: PublicKey,
  pool: PublicKey,
  baseVault: PublicKey,
  quoteVault: PublicKey
): TransactionInstruction {
  const data = Buffer.from([IX_SET_VAULTS]);

  return new TransactionInstruction({
    keys: [
      { pubkey: authority, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: baseVault, isSigner: false, isWritable: false },
      { pubkey: quoteVault, isSigner: false, isWritable: false },
    ],
    programId,
    data,
  });
}

export interface SwapParams {
  direction: number; // 0 = BuyExactIn, 1 = SellExactOut
  amountIn: bigint;
  minAmountOut: bigint;
}

/**
 * Create Swap instruction
 *
 * Accounts:
 * 0. [signer] User
 * 1. [writable] Pool
 * 2. [writable] User Base Account
 * 3. [writable] User Quote Account
 * 4. [writable] Pool Base Vault
 * 5. [writable] Pool Quote Vault
 * 6. [] Base Mint
 * 7. [] Quote Mint
 * 8. [] Base Token Program (SPL Token or Token-2022)
 * 9. [] Quote Token Program (SPL Token or Token-2022)
 */
export function createSwapInstruction(
  programId: PublicKey,
  user: PublicKey,
  pool: PublicKey,
  userBaseAccount: PublicKey,
  userQuoteAccount: PublicKey,
  poolBaseVault: PublicKey,
  poolQuoteVault: PublicKey,
  baseMint: PublicKey,
  quoteMint: PublicKey,
  baseTokenProgram: PublicKey,
  quoteTokenProgram: PublicKey,
  params: SwapParams
): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([IX_SWAP]),
    Buffer.from([params.direction]),
    writeU64LE(params.amountIn),
    writeU64LE(params.minAmountOut),
  ]);

  return new TransactionInstruction({
    keys: [
      { pubkey: user, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: userBaseAccount, isSigner: false, isWritable: true },
      { pubkey: userQuoteAccount, isSigner: false, isWritable: true },
      { pubkey: poolBaseVault, isSigner: false, isWritable: true },
      { pubkey: poolQuoteVault, isSigner: false, isWritable: true },
      { pubkey: baseMint, isSigner: false, isWritable: false },
      { pubkey: quoteMint, isSigner: false, isWritable: false },
      { pubkey: baseTokenProgram, isSigner: false, isWritable: false },
      { pubkey: quoteTokenProgram, isSigner: false, isWritable: false },
    ],
    programId,
    data,
  });
}

export interface DepositWithdrawParams {
  side: number; // 0 = Base (wSOL), 1 = Quote (USDC)
  amount: bigint;
}

/**
 * Create Deposit instruction
 *
 * Accounts:
 * 0. [signer] Authority
 * 1. [writable] Pool
 * 2. [writable] Authority Token Account (source)
 * 3. [writable] Pool Vault (destination)
 * 4. [] Mint
 * 5. [] Token Program (SPL Token or Token-2022)
 */
export function createDepositInstruction(
  programId: PublicKey,
  authority: PublicKey,
  pool: PublicKey,
  authorityTokenAccount: PublicKey,
  poolVault: PublicKey,
  mint: PublicKey,
  tokenProgram: PublicKey,
  params: DepositWithdrawParams
): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([IX_DEPOSIT]),
    Buffer.from([params.side]),
    writeU64LE(params.amount),
  ]);

  return new TransactionInstruction({
    keys: [
      { pubkey: authority, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: authorityTokenAccount, isSigner: false, isWritable: true },
      { pubkey: poolVault, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: tokenProgram, isSigner: false, isWritable: false },
    ],
    programId,
    data,
  });
}

/**
 * Create Withdraw instruction
 *
 * Accounts:
 * 0. [signer] Authority
 * 1. [writable] Pool
 * 2. [writable] Authority Token Account (destination)
 * 3. [writable] Pool Vault (source)
 * 4. [] Mint
 * 5. [] Token Program (SPL Token or Token-2022)
 */
export function createWithdrawInstruction(
  programId: PublicKey,
  authority: PublicKey,
  pool: PublicKey,
  authorityTokenAccount: PublicKey,
  poolVault: PublicKey,
  mint: PublicKey,
  tokenProgram: PublicKey,
  params: DepositWithdrawParams
): TransactionInstruction {
  const data = Buffer.concat([
    Buffer.from([IX_WITHDRAW]),
    Buffer.from([params.side]),
    writeU64LE(params.amount),
  ]);

  return new TransactionInstruction({
    keys: [
      { pubkey: authority, isSigner: true, isWritable: false },
      { pubkey: pool, isSigner: false, isWritable: true },
      { pubkey: authorityTokenAccount, isSigner: false, isWritable: true },
      { pubkey: poolVault, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: tokenProgram, isSigner: false, isWritable: false },
    ],
    programId,
    data,
  });
}

/**
 * Piecewise book side state (72 bytes)
 */
export interface PiecewiseBookSideState {
  prices: bigint[];       // 7 price points (56 bytes)
  totalQuantity: bigint;  // 8 bytes
  consumed: bigint;       // 8 bytes
}

/**
 * Pool account layout (321 bytes)
 *
 * Layout:
 * - discriminator: 8 bytes
 * - bump: 1 byte
 * - authority: 32 bytes
 * - base_mint: 32 bytes
 * - quote_mint: 32 bytes
 * - base_vault: 32 bytes
 * - quote_vault: 32 bytes
 * - bid_side: 72 bytes (PiecewiseBookSide)
 * - ask_side: 72 bytes (PiecewiseBookSide)
 * - is_active: 1 byte
 * - _padding: 7 bytes
 */
export interface PoolState {
  discriminator: string;
  bump: number;
  authority: PublicKey;
  baseMint: PublicKey;
  quoteMint: PublicKey;
  baseVault: PublicKey;
  quoteVault: PublicKey;
  bidSide: PiecewiseBookSideState;
  askSide: PiecewiseBookSideState;
  isActive: boolean;
}

// Byte offsets for pool decoding
const POOL_DISCRIMINATOR_OFFSET = 0;
const POOL_BUMP_OFFSET = 8;
const POOL_AUTHORITY_OFFSET = 9;
const POOL_BASE_MINT_OFFSET = 41;
const POOL_QUOTE_MINT_OFFSET = 73;
const POOL_BASE_VAULT_OFFSET = 105;
const POOL_QUOTE_VAULT_OFFSET = 137;
const POOL_BID_SIDE_OFFSET = 169;
const POOL_ASK_SIDE_OFFSET = 241; // 169 + 72
const POOL_IS_ACTIVE_OFFSET = 313; // 241 + 72

/**
 * Decode a PiecewiseBookSide from buffer at given offset
 */
function decodePiecewiseBookSide(data: Buffer, offset: number): PiecewiseBookSideState {
  const prices: bigint[] = [];
  for (let i = 0; i < NUM_PRICE_POINTS; i++) {
    prices.push(data.readBigUInt64LE(offset + i * 8));
  }
  const totalQuantity = data.readBigUInt64LE(offset + 56);
  const consumed = data.readBigUInt64LE(offset + 64);

  return { prices, totalQuantity, consumed };
}

/**
 * Decode pool account data
 */
export function decodePoolState(data: Buffer): PoolState {
  return {
    discriminator: data.subarray(POOL_DISCRIMINATOR_OFFSET, POOL_DISCRIMINATOR_OFFSET + 8).toString("utf8"),
    bump: data[POOL_BUMP_OFFSET],
    authority: new PublicKey(data.subarray(POOL_AUTHORITY_OFFSET, POOL_AUTHORITY_OFFSET + 32)),
    baseMint: new PublicKey(data.subarray(POOL_BASE_MINT_OFFSET, POOL_BASE_MINT_OFFSET + 32)),
    quoteMint: new PublicKey(data.subarray(POOL_QUOTE_MINT_OFFSET, POOL_QUOTE_MINT_OFFSET + 32)),
    baseVault: new PublicKey(data.subarray(POOL_BASE_VAULT_OFFSET, POOL_BASE_VAULT_OFFSET + 32)),
    quoteVault: new PublicKey(data.subarray(POOL_QUOTE_VAULT_OFFSET, POOL_QUOTE_VAULT_OFFSET + 32)),
    bidSide: decodePiecewiseBookSide(data, POOL_BID_SIDE_OFFSET),
    askSide: decodePiecewiseBookSide(data, POOL_ASK_SIDE_OFFSET),
    isActive: data[POOL_IS_ACTIVE_OFFSET] === 1,
  };
}

/**
 * Format native-ratio price for display
 * Requires token decimals to convert back to human-readable price
 */
export function formatPriceWithDecimals(
  nativePrice: bigint,
  baseDecimals: number,
  quoteDecimals: number
): string {
  const humanPrice = nativeToHumanPrice(nativePrice, baseDecimals, quoteDecimals);
  return `$${humanPrice.toFixed(2)}`;
}

/**
 * Legacy format function - assumes price is already in human-readable scaled form
 * @deprecated Use formatPriceWithDecimals for native-ratio prices
 */
export function formatPrice(scaledPrice: bigint): string {
  const dollars = Number(scaledPrice) / Number(PRICE_SCALE);
  return `$${dollars.toFixed(2)}`;
}

/**
 * Format USDC for display
 */
export function formatUsdc(microUnits: bigint): string {
  const usdc = Number(microUnits) / Number(USDC_SCALE);
  return `${usdc.toFixed(2)} USDC`;
}

/**
 * Format SOL for display
 */
export function formatSol(lamports: bigint): string {
  const sol = Number(lamports) / Number(SOL_SCALE);
  return `${sol.toFixed(4)} SOL`;
}

/**
 * Format any token amount for display
 */
export function formatTokenAmount(
  nativeUnits: bigint,
  decimals: number,
  symbol: string = ""
): string {
  const amount = Number(nativeUnits) / (10 ** decimals);
  const suffix = symbol ? ` ${symbol}` : "";
  return `${amount.toFixed(Math.min(decimals, 8))}${suffix}`;
}

// ============================================================================
// Piecewise Curve Utilities
// ============================================================================

/**
 * Calculate remaining liquidity on a book side
 */
export function getRemainingLiquidity(side: PiecewiseBookSideState): bigint {
  return side.totalQuantity - side.consumed;
}

/**
 * Calculate quantity per segment
 */
export function getQuantityPerSegment(side: PiecewiseBookSideState): bigint {
  return side.totalQuantity / BigInt(NUM_SEGMENTS);
}

/**
 * Get current segment index based on consumption
 * Returns 0-5 for ask side (consuming low to high)
 * Returns 5-0 for bid side (consuming high to low)
 */
export function getCurrentSegment(side: PiecewiseBookSideState): number {
  const qSegment = getQuantityPerSegment(side);
  if (qSegment === 0n) return 0;
  const segIdx = Number(side.consumed / qSegment);
  return Math.min(segIdx, NUM_SEGMENTS - 1);
}

/**
 * Format piecewise book side for display
 */
export function formatPiecewiseBookSide(
  side: PiecewiseBookSideState,
  baseDecimals: number,
  quoteDecimals: number,
  isAskSide: boolean
): string {
  const lines: string[] = [];
  const remaining = getRemainingLiquidity(side);
  const qPerSeg = getQuantityPerSegment(side);
  const currentSeg = getCurrentSegment(side);

  lines.push(`  Total Quantity: ${side.totalQuantity}`);
  lines.push(`  Consumed: ${side.consumed}`);
  lines.push(`  Remaining: ${remaining}`);
  lines.push(`  Current Segment: ${currentSeg}`);
  lines.push(`  Prices:`);

  for (let i = 0; i < NUM_PRICE_POINTS; i++) {
    const price = nativeToHumanPrice(side.prices[i], baseDecimals, quoteDecimals);
    const marker = (isAskSide && i === currentSeg) || (!isAskSide && i === NUM_SEGMENTS - currentSeg)
      ? " <-- current"
      : "";
    lines.push(`    P${i}: $${price.toFixed(4)}${marker}`);
  }

  return lines.join("\n");
}

/**
 * Create evenly-spaced price points for oracle update
 *
 * @param lowerPrice - Human-readable lower price
 * @param upperPrice - Human-readable upper price
 * @param baseDecimals - Base token decimals
 * @param quoteDecimals - Quote token decimals
 * @returns Array of 7 native-ratio prices
 */
export function createEvenlySpacedPrices(
  lowerPrice: number,
  upperPrice: number,
  baseDecimals: number,
  quoteDecimals: number
): bigint[] {
  const prices: bigint[] = [];
  const step = (upperPrice - lowerPrice) / NUM_SEGMENTS;

  for (let i = 0; i <= NUM_SEGMENTS; i++) {
    const humanPrice = lowerPrice + step * i;
    prices.push(humanToNativePrice(humanPrice, baseDecimals, quoteDecimals));
  }

  return prices;
}
