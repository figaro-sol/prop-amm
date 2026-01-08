/**
 * Initialize NVDAX/USDC Piecewise AMM Pool
 *
 * Creates pool with 7-point curves concentrated at the inside:
 * - Bid: $100-$180 (more liquidity near $180)
 * - Ask: $200-$300 (more liquidity near $200)
 *
 * Required environment variables:
 * - SOLANA_RPC_URL: RPC endpoint
 * - SOLANA_PRIVATE_KEY: Base58 encoded private key
 */

import { config } from "dotenv";
import { join } from "path";
config({ path: join(process.cwd(), "..", ".env") });

import bs58 from "bs58";
import { Connection, Keypair, PublicKey, Transaction, sendAndConfirmTransaction } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, getOrCreateAssociatedTokenAccount } from "@solana/spl-token";

import {
  PROP_AMM_PROGRAM_ID,
  createInitializeInstruction,
  createSetVaultsInstruction,
  createUpdateOracleInstruction,
  humanToNativePrice,
} from "./instructions";

// Mints
const NVDAX_MINT = new PublicKey("Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh");
const USDC_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

// Decimals
const NVDAX_DECIMALS = 8;
const USDC_DECIMALS = 6;

// Pool seed
const POOL_SEED = Buffer.from("pool");

function findPoolPda(programId: PublicKey, baseMint: PublicKey, quoteMint: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [POOL_SEED, baseMint.toBuffer(), quoteMint.toBuffer()],
    programId
  );
}

async function main() {
  console.log("=".repeat(60));
  console.log("Initialize NVDAX/USDC Piecewise AMM Pool");
  console.log("=".repeat(60));

  // Load from environment
  const rpcUrl = process.env.SOLANA_RPC_URL;
  const privateKey = process.env.SOLANA_PRIVATE_KEY;
  if (!rpcUrl || !privateKey) {
    throw new Error("SOLANA_RPC_URL and SOLANA_PRIVATE_KEY must be set in .env");
  }

  const connection = new Connection(rpcUrl, "confirmed");
  const authority = Keypair.fromSecretKey(bs58.decode(privateKey));

  console.log(`\nAuthority: ${authority.publicKey.toBase58()}`);
  console.log(`Program ID: ${PROP_AMM_PROGRAM_ID.toBase58()}`);
  console.log(`NVDAX Mint: ${NVDAX_MINT.toBase58()}`);
  console.log(`USDC Mint: ${USDC_MINT.toBase58()}`);

  // Derive pool PDA
  const [poolPda, bump] = findPoolPda(PROP_AMM_PROGRAM_ID, NVDAX_MINT, USDC_MINT);
  console.log(`\nPool PDA: ${poolPda.toBase58()}`);
  console.log(`Pool bump: ${bump}`);

  // Check if pool already exists
  const poolInfo = await connection.getAccountInfo(poolPda);
  if (poolInfo) {
    console.log("\nPool already exists! Skipping initialization...");
  } else {
    // Step 1: Initialize pool
    console.log("\n--- Step 1: Initialize Pool ---");
    const initIx = createInitializeInstruction(
      PROP_AMM_PROGRAM_ID,
      authority.publicKey,
      poolPda,
      NVDAX_MINT,
      USDC_MINT,
      { bump }
    );

    const initTx = new Transaction().add(initIx);
    const initSig = await sendAndConfirmTransaction(connection, initTx, [authority]);
    console.log(`Initialize tx: ${initSig}`);
  }

  // Step 2: Create/Get vault ATAs
  console.log("\n--- Step 2: Create Vault ATAs ---");

  // NVDAX vault (Token-2022)
  const poolBaseVault = await getOrCreateAssociatedTokenAccount(
    connection,
    authority,
    NVDAX_MINT,
    poolPda,
    true, // allowOwnerOffCurve for PDA
    undefined,
    undefined,
    TOKEN_2022_PROGRAM_ID
  );
  console.log(`NVDAX Vault: ${poolBaseVault.address.toBase58()}`);

  // USDC vault (SPL Token)
  const poolQuoteVault = await getOrCreateAssociatedTokenAccount(
    connection,
    authority,
    USDC_MINT,
    poolPda,
    true,
    undefined,
    undefined,
    TOKEN_PROGRAM_ID
  );
  console.log(`USDC Vault: ${poolQuoteVault.address.toBase58()}`);

  // Step 3: Set vaults
  console.log("\n--- Step 3: Set Vaults ---");
  const setVaultsIx = createSetVaultsInstruction(
    PROP_AMM_PROGRAM_ID,
    authority.publicKey,
    poolPda,
    poolBaseVault.address,
    poolQuoteVault.address
  );

  const setVaultsTx = new Transaction().add(setVaultsIx);
  const setVaultsSig = await sendAndConfirmTransaction(connection, setVaultsTx, [authority]);
  console.log(`SetVaults tx: ${setVaultsSig}`);

  // Step 4: Update Oracle with piecewise curves (PRICES ONLY)
  // Note: Oracle does NOT set quantities - they come from deposits only
  console.log("\n--- Step 4: Update Oracle ---");

  // Bid side: $100-$180, more liquidity near $180 (inside)
  // Prices closer together near the top
  const bidPrices = [
    100,  // P0 - far
    115,  // P1 (+15)
    135,  // P2 (+20)
    155,  // P3 (+20)
    168,  // P4 (+13)
    175,  // P5 (+7)
    180,  // P6 (+5) - inside (best bid)
  ].map(p => humanToNativePrice(p, NVDAX_DECIMALS, USDC_DECIMALS));

  // Ask side: $200-$300, more liquidity near $200 (inside)
  // Prices closer together near the bottom
  const askPrices = [
    200,  // P0 - inside (best ask)
    205,  // P1 (+5)
    212,  // P2 (+7)
    225,  // P3 (+13)
    245,  // P4 (+20)
    270,  // P5 (+25)
    300,  // P6 (+30) - far
  ].map(p => humanToNativePrice(p, NVDAX_DECIMALS, USDC_DECIMALS));

  console.log("\nBid curve (USDC to buy NVDAX):");
  bidPrices.forEach((p, i) => {
    const human = Number(p) / 1e9 * (10 ** NVDAX_DECIMALS) / (10 ** USDC_DECIMALS);
    const gap = i > 0 ? human - Number(bidPrices[i-1]) / 1e9 * (10 ** NVDAX_DECIMALS) / (10 ** USDC_DECIMALS) : 0;
    console.log(`  P${i}: $${human.toFixed(0)}${gap > 0 ? ` (+$${gap.toFixed(0)})` : ''}`);
  });

  console.log("\nAsk curve (sell NVDAX for USDC):");
  askPrices.forEach((p, i) => {
    const human = Number(p) / 1e9 * (10 ** NVDAX_DECIMALS) / (10 ** USDC_DECIMALS);
    const gap = i > 0 ? human - Number(askPrices[i-1]) / 1e9 * (10 ** NVDAX_DECIMALS) / (10 ** USDC_DECIMALS) : 0;
    console.log(`  P${i}: $${human.toFixed(0)}${gap > 0 ? ` (+$${gap.toFixed(0)})` : ''}`);
  });

  const updateOracleIx = createUpdateOracleInstruction(
    PROP_AMM_PROGRAM_ID,
    authority.publicKey,
    poolPda,
    { bidPrices, askPrices }
  );

  const oracleTx = new Transaction().add(updateOracleIx);
  const oracleSig = await sendAndConfirmTransaction(connection, oracleTx, [authority]);
  console.log(`\nUpdateOracle tx: ${oracleSig}`);

  console.log("\n" + "=".repeat(60));
  console.log("Pool initialized successfully!");
  console.log("=".repeat(60));
  console.log(`\nPool: ${poolPda.toBase58()}`);
  console.log(`NVDAX Vault: ${poolBaseVault.address.toBase58()}`);
  console.log(`USDC Vault: ${poolQuoteVault.address.toBase58()}`);
  console.log("\nNote: Pool starts inactive with 0 liquidity.");
  console.log("Deposit NVDAX and USDC to enable trading.");
}

main().catch(console.error);
