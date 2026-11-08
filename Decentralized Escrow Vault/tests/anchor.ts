import BN from "bn.js";
import * as web3 from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import {
  PublicKey,
  Keypair,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import type { AnchorEscrow } from "../target/types/anchor_escrow";

function assertEqual(actual: any, expected: any) {
  if (actual.toString() !== expected.toString()) {
    throw new Error(`Expected ${expected.toString()}, but got ${actual.toString()}`);
  }
}

describe("anchor-escrow", () => {
  // Configure the client to use the local cluster
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.AnchorEscrow as anchor.Program<AnchorEscrow>;
  
  const connection = program.provider.connection;
  const program = program;
  const payer = program.provider.wallet.payer;

  const maker = Keypair.generate();
  const taker = Keypair.generate();

  let mintA: PublicKey;
  let mintB: PublicKey;
  let makerAtaA: PublicKey;
  let makerAtaB: PublicKey;
  let takerAtaA: PublicKey;
  let takerAtaB: PublicKey;
  let escrowPda: PublicKey;
  let vaultAta: PublicKey;

  const seed = new anchor.BN(1001);
  const depositAmount = new anchor.BN(500_000_000); // 500 Token A (6 decimals)
  const receiveAmount = new anchor.BN(250_000_000); // 250 Token B (6 decimals)

  it("1. Setup Mints and Token Accounts on Devnet", async () => {
    const fundTx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: payer.publicKey,
        toPubkey: maker.publicKey,
        lamports: 0.05 * LAMPORTS_PER_SOL,
      }),
      SystemProgram.transfer({
        fromPubkey: payer.publicKey,
        toPubkey: taker.publicKey,
        lamports: 0.05 * LAMPORTS_PER_SOL,
      })
    );
    await sendAndConfirmTransaction(connection, fundTx, [payer], {
      commitment: "confirmed",
    });

    mintA = await createMint(
      connection,
      payer,
      payer.publicKey,
      null,
      6,
      undefined,
      { commitment: "confirmed" }
    );
    mintB = await createMint(
      connection,
      payer,
      payer.publicKey,
      null,
      6,
      undefined,
      { commitment: "confirmed" }
    );

    makerAtaA = await createAssociatedTokenAccount(
      connection,
      payer,
      mintA,
      maker.publicKey,
      { commitment: "confirmed" }
    );
    await mintTo(
      connection,
      payer,
      mintA,
      makerAtaA,
      payer,
      1_000_000_000,
      [],
      { commitment: "confirmed" }
    );

    takerAtaB = await createAssociatedTokenAccount(
      connection,
      payer,
      mintB,
      taker.publicKey,
      { commitment: "confirmed" }
    );
    await mintTo(
      connection,
      payer,
      mintB,
      takerAtaB,
      payer,
      1_000_000_000,
      [],
      { commitment: "confirmed" }
    );

    [escrowPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        maker.publicKey.toBuffer(),
        seed.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );

    vaultAta = getAssociatedTokenAddressSync(mintA, escrowPda, true);
    makerAtaB = getAssociatedTokenAddressSync(mintB, maker.publicKey);
    takerAtaA = getAssociatedTokenAddressSync(mintA, taker.publicKey);

    console.log("Setup complete! Escrow PDA:", escrowPda.toBase58());
  });

  it("2. Maker initializes Escrow and deposits Token A into Vault", async () => {
    const txHash = await program.methods
      .make(seed, depositAmount, receiveAmount)
      .accounts({
        maker: maker.publicKey,
        mintA: mintA,
        mintB: mintB,
        makerAtaA: makerAtaA,
        escrow: escrowPda,
        vault: vaultAta,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc({ commitment: "confirmed" });

    await connection.confirmTransaction(txHash, "confirmed");

    const vaultAccount = await getAccount(connection, vaultAta, "confirmed");
    assertEqual(vaultAccount.amount, depositAmount);
    console.log("Vault locked balance (Token A):", vaultAccount.amount.toString());
  });

  it("3. Taker fills the Escrow order and closes the Vault", async () => {
    const txHash = await program.methods
      .take()
      .accounts({
        taker: taker.publicKey,
        maker: maker.publicKey,
        mintA: mintA,
        mintB: mintB,
        takerAtaA: takerAtaA,
        takerAtaB: takerAtaB,
        makerAtaB: makerAtaB,
        escrow: escrowPda,
        vault: vaultAta,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([taker])
      .rpc({ commitment: "confirmed" });

    await connection.confirmTransaction(txHash, "confirmed");

    const takerAccountA = await getAccount(connection, takerAtaA, "confirmed");
    const makerAccountB = await getAccount(connection, makerAtaB, "confirmed");

    assertEqual(takerAccountA.amount, depositAmount);
    assertEqual(makerAccountB.amount, receiveAmount);
    console.log("Swap complete! Taker received Token A:", takerAccountA.amount.toString());
    console.log("Maker received Token B:", makerAccountB.amount.toString());
  });

  it("4. Maker cancels an open Escrow order and withdraws Token A", async () => {
    const cancelSeed = new anchor.BN(1002);
    const cancelDeposit = new anchor.BN(200_000_000);

    const [cancelEscrowPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        maker.publicKey.toBuffer(),
        cancelSeed.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    const cancelVaultAta = getAssociatedTokenAddressSync(mintA, cancelEscrowPda, true);

    await program.methods
      .make(cancelSeed, cancelDeposit, receiveAmount)
      .accounts({
        maker: maker.publicKey,
        mintA: mintA,
        mintB: mintB,
        makerAtaA: makerAtaA,
        escrow: cancelEscrowPda,
        vault: cancelVaultAta,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc({ commitment: "confirmed" });

    const txHash = await program.methods
      .refund()
      .accounts({
        maker: maker.publicKey,
        mintA: mintA,
        makerAtaA: makerAtaA,
        escrow: cancelEscrowPda,
        vault: cancelVaultAta,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc({ commitment: "confirmed" });

    await connection.confirmTransaction(txHash, "confirmed");
    console.log("Escrow cancelled and refunded successfully! Tx:", txHash);
  });
});