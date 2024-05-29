import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorError } from "@coral-xyz/anchor";
import { TokenVesting } from "../target/types/token_vesting";
import * as spl from "@solana/spl-token";
import * as assert from "assert";
import {
  createMint,
  createUserAndATA,
  fundATA,
  getTokenBalanceWeb3,
  createPDA,
} from "./utils";

describe("token_vesting", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  // 공급자 env확인
  console.log(provider);
  const program = anchor.workspace.TokenVesting as Program<TokenVesting>;

  let mintAddress,
    sender,
    senderATA,
    dataAccount,
    dataBump,
    escrowWallet,
    escrowBump,
    beneficiary,
    beneficiaryATA,
    beneficiaryArray,
    decimals;

  let _dataAccountAfterInit, _dataAccountAfterRelease, _dataAccountAfterClaim; // Used to store State between tests

  before(async () => {
    decimals = 1;
    mintAddress = await createMint(provider, decimals);
    [sender, senderATA] = await createUserAndATA(provider, mintAddress);
    await fundATA(provider, mintAddress, sender, senderATA, decimals);

    // Create PDA's for account_data_account and escrow_wallet
    [dataAccount, dataBump] = await createPDA(
      [Buffer.from("lucia_data_account"), mintAddress.toBuffer()],
      program.programId,
    );

    [escrowWallet, escrowBump] = await createPDA(
      [Buffer.from("lucia_escrow_wallet"), mintAddress.toBuffer()],
      program.programId,
    );

    // Create a test Beneficiary object to send into contract
    [beneficiary, beneficiaryATA] = await createUserAndATA(
      provider,
      mintAddress,
    );

    beneficiaryArray = [
      {
        key: beneficiary.publicKey,
        allocatedTokens: new anchor.BN(100000000),
        claimedTokens: new anchor.BN(0),
        initial_bonus_percentage: 0.0, // f32
        lockup_period: new anchor.BN(12 * 30 * 24 * 60 * 60), // i64 (12 months in seconds)
        release_period: new anchor.BN(12 * 30 * 24 * 60 * 60), // i64 (12 months in seconds)
      },
    ];
  });

  it("Test Initialize", async () => {
    // Send initialize transaction
    const initTx = await program.methods
      .initialize(beneficiaryArray, new anchor.BN(1000000000), decimals)
      .accounts({
        dataAccount: dataAccount,
        escrowWallet: escrowWallet,
        walletToWithdrawFrom: senderATA,
        tokenMint: mintAddress,
        sender: sender.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
      })
      .signers([sender])
      .rpc();

    let accountAfterInit = await program.account.dataAccount.fetch(dataAccount);
    // assert.ok(accountAfterInit.beneficiaries[0].lockupPeriod.toNumber() > 0);
    // console.log(
    //   "Lockup end time:",
    //   new Date(accountAfterInit.beneficiaries[0].lockupPeriod.toNumber() * 1000),
    // );

    console.log(
      `init TX: https://explorer.solana.com/tx/${initTx}?cluster=custom`,
    );

    assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 1000000000); // Escrow account receives balance of token
    assert.equal(accountAfterInit.beneficiaries[0].allocatedTokens, 100000000); // Tests allocatedTokens field

    _dataAccountAfterInit = dataAccount;
  });

  // it("Test Release With False Sender", async () => {
  //   dataAccount = _dataAccountAfterInit;

  //   const falseSender = anchor.web3.Keypair.generate();
  //   try {
  //     const releaseTx = await program.methods
  //       .releaseLuciaVesting(dataBump, 100, 10.0, 10.0)
  //       .accounts({
  //         dataAccount: dataAccount,
  //         sender: falseSender.publicKey,
  //         tokenMint: mintAddress,
  //         systemProgram: anchor.web3.SystemProgram.programId,
  //       })
  //       .signers([falseSender])
  //       .rpc();
  //     assert.ok(false, "Error was supposed to be thrown");
  //   } catch (err) {
  //     // console.log(err);
  //     assert.equal(err instanceof AnchorError, true);
  //     assert.equal(err.error.errorCode.code, "InvalidSender");
  //   }
  // });

  // it("Test Release", async () => {
  //   dataAccount = _dataAccountAfterInit;

  //   const releaseTx = await program.methods
  //     .releaseLuciaVesting(dataBump, 100, 10.0, 10.0)
  //     .accounts({
  //       dataAccount: dataAccount,
  //       sender: sender.publicKey,
  //       tokenMint: mintAddress,
  //       systemProgram: anchor.web3.SystemProgram.programId,
  //     })
  //     .signers([sender])
  //     .rpc();

  //   let accountAfterRelease =
  //     await program.account.dataAccount.fetch(dataAccount);
  //   console.log(
  //     `release TX: https://explorer.solana.com/tx/${releaseTx}?cluster=custom`,
  //   );

  //   assert.equal(accountAfterRelease.percentAvailable, 100); // Percent Available updated correctly
  //   assert.equal(accountAfterRelease.baseClaimPercentage, 10.0); // 기본 비율 확인
  //   assert.equal(accountAfterRelease.initialBonusPercentage, 10.0); // 첫 번째 달 추가 비율 확인

  //   _dataAccountAfterRelease = dataAccount;
  // });

  // it("Test First Month Claim", async () => {
  //   dataAccount = _dataAccountAfterRelease;

  //   // Set up a mock time to simulate the first month after lockup end
  //   const lockupEndTime =
  //     Math.floor(Date.now() / 1000) - 3 * 365 * 24 * 60 * 60; // 3 years ago
  //   const firstMonthTime = new anchor.BN(lockupEndTime).add(
  //     new anchor.BN(3 * 365 * 24 * 60 * 60 + 30 * 24 * 60 * 60),
  //   ); // 3 years and 30 days later

  //   // Simulate time check for the first month
  //   const currentTime = new anchor.BN(Math.floor(Date.now() / 1000));
  //   if (currentTime.lt(firstMonthTime)) {
  //     throw new Error(
  //       "Current time is not within the first month after lockup end",
  //     );
  //   }

  //   const claimTx = await program.methods
  //     .claimLuciaToken(dataBump, escrowBump)
  //     .accounts({
  //       dataAccount: dataAccount,
  //       escrowWallet: escrowWallet,
  //       sender: beneficiary.publicKey,
  //       tokenMint: mintAddress,
  //       walletToDepositTo: beneficiaryATA,
  //       associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
  //       tokenProgram: spl.TOKEN_PROGRAM_ID,
  //       systemProgram: anchor.web3.SystemProgram.programId,
  //     })
  //     .signers([beneficiary])
  //     .rpc();
  //   console.log(
  //     `claim TX: https://explorer.solana.com/tx/${claimTx}?cluster=custom`,
  //   );

  //   assert.equal(await getTokenBalanceWeb3(beneficiaryATA, provider), 100); // 예시: 첫 번째 달 클레임 후 잔액 확인
  //   assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 900); // 예시: 첫 번째 달 클레임 후 에스크로 잔액 확인

  //   _dataAccountAfterClaim = dataAccount;
  // });

  // it("Test Second Month Claim", async () => {
  //   dataAccount = _dataAccountAfterClaim;

  //   // Set up a mock time to simulate the first month after lockup end
  //   const lockupEndTime =
  //     Math.floor(Date.now() / 1000) - 3 * 365 * 24 * 60 * 60; // 3 years ago
  //   const secondMonthTime = new anchor.BN(lockupEndTime).add(
  //     new anchor.BN(3 * 365 * 24 * 60 * 60 + 60 * 24 * 60 * 60),
  //   ); // 3 years and 30 days later

  //   // Simulate time check for the first month
  //   const currentTime = new anchor.BN(Math.floor(Date.now() / 1000));
  //   if (currentTime.lt(secondMonthTime)) {
  //     throw new Error(
  //       "Current time is not within the first month after lockup end",
  //     );
  //   }

  //   const claimTx = await program.methods
  //     .claimLuciaToken(dataBump, escrowBump)
  //     .accounts({
  //       dataAccount: dataAccount,
  //       escrowWallet: escrowWallet,
  //       sender: beneficiary.publicKey,
  //       tokenMint: mintAddress,
  //       walletToDepositTo: beneficiaryATA,
  //       associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
  //       tokenProgram: spl.TOKEN_PROGRAM_ID,
  //       systemProgram: anchor.web3.SystemProgram.programId,
  //     })
  //     .signers([beneficiary])
  //     .rpc();
  //   console.log(
  //     `claim TX: https://explorer.solana.com/tx/${claimTx}?cluster=custom`,
  //   );

  //   console.log();
  //   assert.equal(await getTokenBalanceWeb3(beneficiaryATA, provider), 100); // 예시: 두 번째 달 클레임 후 잔액 확인
  //   assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 900); // 예시: 두 번째 달 클레임 후 에스크로 잔액 확인

  //   _dataAccountAfterClaim = dataAccount;
  // });

  // it("Test Claim", async () => {
  //   // Send initialize transaction
  //   dataAccount = _dataAccountAfterRelease;

  //   const claimTx = await program.methods
  //     .claimLuciaToken(dataBump, escrowBump)
  //     .accounts({
  //       dataAccount: dataAccount,
  //       escrowWallet: escrowWallet,
  //       sender: beneficiary.publicKey,
  //       tokenMint: mintAddress,
  //       walletToDepositTo: beneficiaryATA,
  //       associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
  //       tokenProgram: spl.TOKEN_PROGRAM_ID,
  //       systemProgram: anchor.web3.SystemProgram.programId,
  //     })
  //     .signers([beneficiary])
  //     .rpc();
  //   console.log(
  //     `claim TX: https://explorer.solana.com/tx/${claimTx}?cluster=custom`,
  //   );

  //   assert.equal(await getTokenBalanceWeb3(beneficiaryATA, provider), 100); // Claim releases 43% of 100 tokens into beneficiary's account
  //   assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 900);

  //   _dataAccountAfterClaim = dataAccount;
  // });

  // it("Test Double Claim (Should Fail)", async () => {
  //   dataAccount = _dataAccountAfterClaim;
  //   try {
  //     // Should fail
  //     const doubleClaimTx = await program.methods
  //       .claimLuciaToken(dataBump, escrowBump)
  //       .accounts({
  //         dataAccount: dataAccount,
  //         escrowWallet: escrowWallet,
  //         sender: beneficiary.publicKey,
  //         tokenMint: mintAddress,
  //         walletToDepositTo: beneficiaryATA,
  //         associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
  //         tokenProgram: spl.TOKEN_PROGRAM_ID,
  //         systemProgram: anchor.web3.SystemProgram.programId,
  //       })
  //       .signers([beneficiary])
  //       .rpc();
  //     assert.ok(false, "Error was supposed to be thrown");
  //   } catch (err) {
  //     assert.equal(err instanceof AnchorError, true);
  //     assert.equal(err.error.errorCode.code, "ClaimNotAllowed");
  //     assert.equal(await getTokenBalanceWeb3(beneficiaryATA, provider), 100);
  //     // Check that error is thrown, that it's the ClaimNotAllowed error, and that the beneficiary's balance has not changed
  //   }
  // });

  // it("Test Beneficiary Not Found (Should Fail)", async () => {
  //   dataAccount = _dataAccountAfterClaim;
  //   try {
  //     // const falseBeneficiary = anchor.web3.Keypair.generate();
  //     const [falseBeneficiary, falseBeneficiaryATA] = await createUserAndATA(
  //       provider,
  //       mintAddress,
  //     );

  //     const benNotFound = await program.methods
  //       .claimLuciaToken(dataBump, escrowBump)
  //       .accounts({
  //         dataAccount: dataAccount,
  //         escrowWallet: escrowWallet,
  //         sender: falseBeneficiary.publicKey,
  //         tokenMint: mintAddress,
  //         walletToDepositTo: falseBeneficiaryATA,
  //         associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
  //         tokenProgram: spl.TOKEN_PROGRAM_ID,
  //         systemProgram: anchor.web3.SystemProgram.programId,
  //       })
  //       .signers([falseBeneficiary])
  //       .rpc();
  //   } catch (err) {
  //     assert.equal(err instanceof AnchorError, true);
  //     assert.equal(err.error.errorCode.code, "BeneficiaryNotFound");
  //   }
  // });
});
