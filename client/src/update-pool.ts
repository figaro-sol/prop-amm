import { config } from "dotenv";
import { join } from "path";
config({ path: join(process.cwd(), "..", ".env") });

import bs58 from "bs58";
import {
  Connection,
  Keypair,
  PublicKey,
  sendAndConfirmTransaction,
  Transaction,
} from "@solana/web3.js";
import {
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
} from "@solana/spl-token";

import {
  createUpdateOracleInstruction,
  createDepositInstruction,
  humanToNativePrice,
  USDC_SCALE,
} from "./instructions.js";

// Create concentrated prices - more liquidity near the best price
// exponent > 1 concentrates near best price
function createConcentratedPrices(
  lowerPrice: number,
  upperPrice: number,
  baseDecimals: number,
  quoteDecimals: number,
  concentrateAtUpper: boolean, // true for bid (best at upper), false for ask (best at lower)
  exponent: number = 2.0
): bigint[] {
  const prices: bigint[] = [];
  const range = upperPrice - lowerPrice;

  for (let i = 0; i <= 6; i++) {
    const t = i / 6; // 0 to 1
    let adjustedT: number;

    if (concentrateAtUpper) {
      // Concentrate near upper (bid side - best price is $180)
      // Tight spacing near t=1, wide spacing near t=0
      adjustedT = 1 - Math.pow(1 - t, exponent);
    } else {
      // Concentrate near lower (ask side - best price is $200)
      // Tight spacing near t=0, wide spacing near t=1
      adjustedT = Math.pow(t, exponent);
    }

    const humanPrice = lowerPrice + adjustedT * range;
    prices.push(humanToNativePrice(humanPrice, baseDecimals, quoteDecimals));
  }

  return prices;
}

// Program ID (deployed)
const PROGRAM_ID = new PublicKey("13NnzndP9KHoqygdU6Bba1cy4EayKN3F3AX6F3PSALnK");

// Pool and token addresses
const POOL_ADDRESS = new PublicKey("Fzi3DDEKikx97rt7HoaGzBhVEu4wGTuMap54kEHtZGJz");
const POOL_BASE_VAULT = new PublicKey("2KzXaWcb7tRSfzsDtSyYL9fBtTE2K7ZvST1Urhn59ynx");
const POOL_QUOTE_VAULT = new PublicKey("772p1LsTkqJgRD4Rf4qAyonCvUogM9h52JsD3AJUbv2x");
const NVDAX_MINT = new PublicKey("Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh");
const USDC_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

// Token decimals
const NVDAX_DECIMALS = 8;
const USDC_DECIMALS = 6;

// Deposit sides (from program: Base=0 affects ask, Quote=1 affects bid)
const SIDE_BASE = 0;  // NVDAX - affects ask side
const SIDE_QUOTE = 1; // USDC - affects bid side

async function main() {
  console.log("=".repeat(60));
  console.log("Update NVDA/USDC Pool Oracle and Deposit");
  console.log("=".repeat(60));

  // Load from environment
  const rpcUrl = process.env.SOLANA_RPC_URL;
  const privateKey = process.env.SOLANA_PRIVATE_KEY;
  if (!rpcUrl || !privateKey) {
    throw new Error("SOLANA_RPC_URL and SOLANA_PRIVATE_KEY must be set in .env");
  }

  const authority = Keypair.fromSecretKey(bs58.decode(privateKey));
  console.log(`Authority: ${authority.publicKey.toBase58()}`);

  // Connect
  const connection = new Connection(rpcUrl, "confirmed");
  const balance = await connection.getBalance(authority.publicKey);
  console.log(`Balance: ${balance / 1e9} SOL`);

  // === UPDATE ORACLE ===
  console.log("\n--- Updating Oracle Prices (Concentrated) ---");

  // Bid side: $100-$180, concentrated near $180 (best bid)
  const bidPrices = createConcentratedPrices(100, 180, NVDAX_DECIMALS, USDC_DECIMALS, true, 2.0);
  const bidHumanPrices = bidPrices.map(p => (Number(p) / 1e9 * 100).toFixed(0));
  console.log("Bid prices ($):", bidHumanPrices.join(", "));

  // Ask side: $200-$300, concentrated near $200 (best ask)
  const askPrices = createConcentratedPrices(200, 300, NVDAX_DECIMALS, USDC_DECIMALS, false, 2.0);
  const askHumanPrices = askPrices.map(p => (Number(p) / 1e9 * 100).toFixed(0));
  console.log("Ask prices ($):", askHumanPrices.join(", "));

  const updateOracleIx = createUpdateOracleInstruction(
    PROGRAM_ID,
    authority.publicKey,
    POOL_ADDRESS,
    { bidPrices, askPrices }
  );

  const updateOracleTx = new Transaction().add(updateOracleIx);
  const updateOracleSig = await sendAndConfirmTransaction(connection, updateOracleTx, [authority]);
  console.log(`Update Oracle tx: ${updateOracleSig}`);

  // === DEPOSIT TO TOP UP (skip if just updating curve shape) ===
  const SKIP_DEPOSITS = true;
  if (SKIP_DEPOSITS) {
    console.log("\n--- Skipping deposits (SKIP_DEPOSITS=true) ---");
    console.log("\n" + "=".repeat(60));
    console.log("Pool Oracle Updated Successfully!");
    console.log("=".repeat(60));
    console.log("Concentrated prices: more liquidity near best bid/ask");
    return;
  }

  console.log("\n--- Depositing to top up liquidity ---");

  // Get authority's token accounts
  const authorityUsdcAccount = getAssociatedTokenAddressSync(
    USDC_MINT,
    authority.publicKey,
    false,
    TOKEN_PROGRAM_ID
  );
  const authorityNvdaxAccount = getAssociatedTokenAddressSync(
    NVDAX_MINT,
    authority.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID
  );

  // Check current balances
  const usdcBalance = await connection.getTokenAccountBalance(authorityUsdcAccount);
  const nvdaxBalance = await connection.getTokenAccountBalance(authorityNvdaxAccount);
  console.log(`Authority USDC balance: ${usdcBalance.value.uiAmount}`);
  console.log(`Authority NVDAX balance: ${nvdaxBalance.value.uiAmount}`);

  // Deposit ~$250 USDC to bid side (Quote token = side 1)
  const usdcDepositAmount = BigInt(250 * 10 ** USDC_DECIMALS); // 250 USDC
  console.log(`\nDepositing ${Number(usdcDepositAmount) / 10 ** USDC_DECIMALS} USDC to bid side...`);

  const depositBidIx = createDepositInstruction(
    PROGRAM_ID,
    authority.publicKey,
    POOL_ADDRESS,
    authorityUsdcAccount,
    POOL_QUOTE_VAULT,
    USDC_MINT,
    TOKEN_PROGRAM_ID,
    { side: SIDE_QUOTE, amount: usdcDepositAmount }
  );

  const depositBidTx = new Transaction().add(depositBidIx);
  const depositBidSig = await sendAndConfirmTransaction(connection, depositBidTx, [authority]);
  console.log(`Deposit bid tx: ${depositBidSig}`);

  // Deposit NVDAX to ask side (Base token = side 0)
  // At avg price of $250, ~$250 worth of NVDAX is ~1 NVDAX
  const nvdaxDepositAmount = BigInt(1 * 10 ** NVDAX_DECIMALS); // 1 NVDAX (~$250 worth)
  console.log(`\nDepositing ${Number(nvdaxDepositAmount) / 10 ** NVDAX_DECIMALS} NVDAX to ask side...`);

  const depositAskIx = createDepositInstruction(
    PROGRAM_ID,
    authority.publicKey,
    POOL_ADDRESS,
    authorityNvdaxAccount,
    POOL_BASE_VAULT,
    NVDAX_MINT,
    TOKEN_2022_PROGRAM_ID,
    { side: SIDE_BASE, amount: nvdaxDepositAmount }
  );

  const depositAskTx = new Transaction().add(depositAskIx);
  const depositAskSig = await sendAndConfirmTransaction(connection, depositAskTx, [authority]);
  console.log(`Deposit ask tx: ${depositAskSig}`);

  console.log("\n" + "=".repeat(60));
  console.log("Pool Updated Successfully!");
  console.log("=".repeat(60));
  console.log("Oracle prices: Bid $100-$180, Ask $200-$300");
  console.log("Deposited: ~$250 USDC (bid), ~1 NVDAX (ask)");
}

main().catch(console.error);
