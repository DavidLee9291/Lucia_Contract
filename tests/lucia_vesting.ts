import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorError } from "@coral-xyz/anchor";
import { LuciaVesting } from "../target/types/lucia_vesting";
import * as spl from '@solana/spl-token';
import * as assert from "assert";
import { createMint, createUserAndATA, fundATA, getTokenBalanceWeb3, createPDA } from "./utils";

describe("lucia_vesting", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.LuciaVesting as Program<LuciaVesting>;

  let mintAddress, sender, senderATA, dataAccount, dataBump, escrowWallet, escrowBump, beneficiary, beneficiaryATA, beneficiaryArray, decimals;

  let _dataAccountAfterInit, _dataAccountAfterRelease, _dataAccountAfterClaim; // Used to store State between tests
 
  before(async () => {
    decimals = 6;
    mintAddress = await createMint(provider, decimals);
    [sender, senderATA] = await createUserAndATA(provider, mintAddress);
    await fundATA(provider, mintAddress, sender, senderATA, decimals);

    // Create PDA's for account_data_account and escrow_wallet
    [dataAccount, dataBump] = await createPDA([Buffer.from("data_account"), mintAddress.toBuffer()], program.programId);
    [escrowWallet, escrowBump] = await createPDA([Buffer.from("escrow_wallet"), mintAddress.toBuffer()], program.programId);

    // Create a test Beneficiary object to send into contract
    [beneficiary, beneficiaryATA] = await createUserAndATA(provider, mintAddress);
     beneficiaryArray = [
      {
        key: beneficiary.publicKey,
        allocatedTokens: new anchor.BN(100),
        claimedTokens: new anchor.BN(0),
        unlockTge: 10.0, // f32
        lockupPeriod:new anchor.BN(12 * 30 * 24 * 60 * 60), // u64 (12 months in seconds)
        unlockDuration: new anchor.BN(12 * 30 * 24 * 60 * 60), // u64 (12 months in seconds)
      },
    ];
  });

  it("Test Initialize", async () => {
    // Send initialize transaction  
    const initTx = await program.methods.initialize(beneficiaryArray, new anchor.BN(1000), decimals).accounts({
      dataAccount: dataAccount,
      escrowWallet: escrowWallet,
      walletToWithdrawFrom: senderATA,
      tokenMint: mintAddress,
      sender: sender.publicKey,
      systemProgram: anchor.web3.SystemProgram.programId,
      tokenProgram: spl.TOKEN_PROGRAM_ID,
    }).signers([sender]).rpc();

    let accountAfterInit = await program.account.dataAccount.fetch(dataAccount);
    
    console.log(`init TX: https://explorer.solana.com/tx/${initTx}?cluster=custom`)

    assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 1000); // Escrow account receives balance of token
    assert.equal(accountAfterInit.beneficiaries[0].allocatedTokens, 100); // Tests allocatedTokens field
    

    _dataAccountAfterInit = dataAccount;

  });

  it("Test Release With False Sender" , async () => {
    dataAccount = _dataAccountAfterInit;

    const falseSender = anchor.web3.Keypair.generate();
    try {
      const releaseTx = await program.methods.releaseLuciaVesting(dataBump, 1).accounts({
        dataAccount: dataAccount,
        sender: falseSender.publicKey,
        tokenMint: mintAddress,
        systemProgram: anchor.web3.SystemProgram.programId,
      }).signers([falseSender]).rpc();
      assert.ok(false, "Error was supposed to be thrown");
    }catch(err) {
      // console.log(err);
      assert.equal(err instanceof AnchorError, true);
      assert.equal(err.error.errorCode.code, "InvalidSender");
    }


  });

  it("Test Release", async () => {
    dataAccount = _dataAccountAfterInit;

    // Release Lucia Vesting
    const releaseTx = await program.methods.releaseLuciaVesting(dataBump, 1).accounts({
        dataAccount: dataAccount,
        sender: sender.publicKey,
        tokenMint: mintAddress,
        systemProgram: anchor.web3.SystemProgram.programId,
    }).signers([sender]).rpc();

    // Fetch the updated data account
    let accountAfterRelease = await program.account.dataAccount.fetch(dataAccount);

    // Check if the percent available has been updated correctly
    console.log(`release TX: https://explorer.solana.com/tx/${releaseTx}?cluster=custom`)
    assert.equal(accountAfterRelease.state, 1); // Check if the state has been updated correctly

    _dataAccountAfterRelease = dataAccount;
});

  it("Test Claim", async () => {
    // Send initialize transaction  
    dataAccount = _dataAccountAfterRelease;

    // Claim tokens
    const claimTx = await program.methods.claimLux(dataBump, escrowBump).accounts({
      dataAccount: dataAccount,
      escrowWallet: escrowWallet,
      sender: beneficiary.publicKey,
      tokenMint: mintAddress,
      walletToDepositTo: beneficiaryATA,
      associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
      tokenProgram: spl.TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId
    }).signers([beneficiary]).rpc();
    console.log(`claim TX: https://explorer.solana.com/tx/${claimTx}?cluster=custom`);

    // Wait for the claim transaction to confirm
    await provider.connection.confirmTransaction(claimTx);

    // Retrieve the token balance of the beneficiary's ATA
    const beneficiaryBalance = await getTokenBalanceWeb3(beneficiaryATA, provider);

    // Check if the claimed tokens match the expected amount
    const expectedClaimAmount = 1; // Expected claim amount
    assert.equal(beneficiaryBalance, expectedClaimAmount, "Beneficiary's token balance does not match expected claim amount");

    // Assert that the escrow wallet's token balance has been updated correctly
    const escrowBalance = await getTokenBalanceWeb3(escrowWallet, provider);
    assert.equal(escrowBalance,30, "Escrow wallet's token balance is incorrect");

    _dataAccountAfterClaim = dataAccount;
});


  it("Test Double Claim (Should Fail)", async () => {
    dataAccount = _dataAccountAfterClaim;
    try {
      // Should fail
      const doubleClaimTx = await program.methods.claimLux(dataBump, escrowBump).accounts({
        dataAccount: dataAccount,
        escrowWallet: escrowWallet,
        sender: beneficiary.publicKey,
        tokenMint: mintAddress,
        walletToDepositTo: beneficiaryATA,
        associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId
      }).signers([beneficiary]).rpc();
      assert.ok(false, "Error was supposed to be thrown");
    } catch (err) {
      assert.equal(err instanceof AnchorError, true);
      assert.equal(err.error.errorCode.code, "ClaimNotAllowed");
      assert.equal(await getTokenBalanceWeb3(beneficiaryATA, provider), 1);
      // Check that error is thrown, that it's the ClaimNotAllowed error, and that the beneficiary's balance has not changed
    }
  });
  it("Test Beneficiary Not Found (Should Fail)", async () => {
    dataAccount = _dataAccountAfterClaim;
    try {
      // const falseBeneficiary = anchor.web3.Keypair.generate();
      const [falseBeneficiary, falseBeneficiaryATA] = await createUserAndATA(provider, mintAddress);

      const benNotFound = await program.methods.claimLux(dataBump, escrowBump).accounts({
        dataAccount: dataAccount,
        escrowWallet: escrowWallet,
        sender: falseBeneficiary.publicKey,
        tokenMint: mintAddress,
        walletToDepositTo: falseBeneficiaryATA,
        associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId
      }).signers([falseBeneficiary]).rpc();

    }catch(err) {
      assert.equal(err instanceof AnchorError, true);
      assert.equal(err.error.errorCode.code, "BeneficiaryNotFound");
    }
  });
});