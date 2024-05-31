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
    decimals = 1;
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
        initial_bonus_percentage: 0.0, // f32
        lockup_period: new anchor.BN(12 * 30 * 24 * 60 * 60), // i64 (12 months in seconds)
        release_period: new anchor.BN(12 * 30 * 24 * 60 * 60), // i64 (12 months in seconds)
      },
    ];
  });

    // 테스트 케이스
    it("Test Initialize", async () => {
        // 트랜잭션 초기화
        const initTx = await program.methods
            .initialize(beneficiaryArray, new anchor.BN(1000), decimals)
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

        console.log(
            `init TX: https://explorer.solana.com/tx/${initTx}?cluster=custom`
        );

        assert.equal(await getTokenBalanceWeb3(escrowWallet, provider), 1000);
        assert.equal(accountAfterInit.beneficiaries[0].allocatedTokens.toNumber(), 100);

        _dataAccountAfterInit = dataAccount;
    });
  
    // Test Release With False Sender
    it("Test Release With False Sender", async () => {
        dataAccount = _dataAccountAfterInit;

        const falseSender = anchor.web3.Keypair.generate();
        try {
            const releaseTx = await program.methods
                .releaseLuciaVesting(dataBump, 1)
                .accounts({
                    dataAccount: dataAccount,
                    sender: falseSender.publicKey,
                    tokenMint: mintAddress,
                    systemProgram: anchor.web3.SystemProgram.programId,
                })
                .signers([falseSender])
                .rpc();
            assert.ok(false, "Error was supposed to be thrown");
        } catch (err) {
            assert.equal(err instanceof AnchorError, true);
            assert.equal(err.error.errorCode.code, "InvalidSender");
        }
    });

    // Test Release
    it("Test Release", async () => {
        dataAccount = _dataAccountAfterInit;

        const releaseTx = await program.methods
            .releaseLuciaVesting(dataBump, 1)
            .accounts({
                dataAccount: dataAccount,
                sender: sender.publicKey,
                tokenMint: mintAddress,
                systemProgram: anchor.web3.SystemProgram.programId,
            })
            .signers([sender])
            .rpc();

        let accountAfterRelease = await program.account.dataAccount.fetch(
            dataAccount
        );
        console.log(
            `release TX: https://explorer.solana.com/tx/${releaseTx}?cluster=custom`
        );

        assert.equal(accountAfterRelease.state, 1);

        _dataAccountAfterRelease = dataAccount;
    });

   // Test Claim
    it("Test Claim", async () => {
        dataAccount = _dataAccountAfterRelease;

        const claimTx = await program.methods
            .claimLuxToken(dataBump, escrowBump)
            .accounts({
                dataAccount: dataAccount,
                escrowWallet: escrowWallet,
                sender: beneficiary.publicKey,
                tokenMint: mintAddress,
                walletToDepositTo: beneficiaryATA,
                associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
                tokenProgram: spl.TOKEN_PROGRAM_ID,
                systemProgram: anchor.web3.SystemProgram.programId,
            })
            .signers([beneficiary])
            .rpc();

        console.log(
            `claim TX: https://explorer.solana.com/tx/${claimTx}?cluster=custom`
        );

        // Get the token balance of beneficiary's ATA
        const beneficiaryBalance = await getTokenBalanceWeb3(
            beneficiaryATA,
            provider
        );
        console.log(`Beneficiary Token Balance: ${beneficiaryBalance}`);

        // Get the token balance of escrow wallet
        const escrowBalance = await getTokenBalanceWeb3(
            escrowWallet.publicKey,
            provider
        );
        console.log(`Escrow Wallet Token Balance: ${escrowBalance}`);

        // Expected balances
        const expectedTgeTokens = 1000; // 10% of 1000 tokens
        const expectedUnlockTokens = 100; // Assuming 9 months since lockup end, (1000 - 100) / 12 * 9
        const expectedBeneficiaryBalance =
            expectedTgeTokens + expectedUnlockTokens;
        const expectedEscrowBalance =
            1000 - expectedBeneficiaryBalance;

        // Assertions
        assert.equal(beneficiaryBalance, expectedBeneficiaryBalance);
        assert.equal(escrowBalance, expectedEscrowBalance);

        _dataAccountAfterClaim = dataAccount;
    });


    // Test Double Claim (Should Fail)
    it("Test Double Claim (Should Fail)", async () => {
        dataAccount = _dataAccountAfterClaim;
        try {
            // Should fail
            const doubleClaimTx = await program.methods
                .claimLuxToken(dataBump, escrowBump)
                .accounts({
                    dataAccount: dataAccount,
                    escrowWallet: escrowWallet,
                    sender: beneficiary.publicKey,
                    tokenMint: mintAddress,
                    walletToDepositTo: beneficiaryATA,
                    associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
                    tokenProgram: spl.TOKEN_PROGRAM_ID,
                    systemProgram: anchor.web3.SystemProgram.programId,
                })
                .signers([beneficiary])
                .rpc();
            assert.ok(false, "Error was supposed to be thrown");
        } catch (err) {
            assert.equal(err instanceof AnchorError, true);
            assert.equal(
                err.error.errorCode.code,
                "ClaimNotAllowed"
            );
            assert.equal(
                await getTokenBalanceWeb3(beneficiaryATA, provider),
                17500000
            );
        }
    });

    // Test Beneficiary Not Found (Should Fail)
    it("Test Beneficiary Not Found (Should Fail)", async () => {
        dataAccount = _dataAccountAfterClaim;
        try {
            const [falseBeneficiary, falseBeneficiaryATA] =
                await createUserAndATA(provider, mintAddress);

            const benNotFound = await program.methods
                .claimLuxToken(dataBump, escrowBump)
                .accounts({
                    dataAccount: dataAccount,
                    escrowWallet: escrowWallet,
                    sender: falseBeneficiary.publicKey,
                    tokenMint: mintAddress,
                    walletToDepositTo: falseBeneficiaryATA,
                    associatedTokenProgram: spl.ASSOCIATED_TOKEN_PROGRAM_ID,
                    tokenProgram: spl.TOKEN_PROGRAM_ID,
                    systemProgram: anchor.web3.SystemProgram.programId,
                })
                .signers([falseBeneficiary])
                .rpc();
            assert.ok(false, "Error was supposed to be thrown");
        } catch (err) {
            assert.equal(err instanceof AnchorError, true);
            assert.equal(
                err.error.errorCode.code,
                "BeneficiaryNotFound"
            );
        }
    });
});