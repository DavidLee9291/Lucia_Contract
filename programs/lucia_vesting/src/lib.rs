use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount, Transfer };

mod calculate;

declare_id!("5ZKdCPumGbSRB7f48GKSDPhwiZzkjMMh6XBpkuHCeLm6");

#[program]
pub mod lucia_vesting {
    use super::*;

    use calculate::*;

    // Initialize function to set up the vesting contract
    pub fn initialize(
        ctx: Context<Initialize>,
        beneficiaries: Vec<Beneficiary>,
        amount: u64,
        decimals: u8
    ) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        // LCD - 05
        if data_account.is_initialized == 1 {
            return Err(VestingError::AlreadyInitialized.into());
        }

        // LCD - 04
        data_account.time_lock_end = Clock::get()?.unix_timestamp + 48 * 60 * 60;

        msg!("Initializing data account with amount: {}, decimals: {}", amount, decimals);
        msg!("Beneficiaries: {:?}", beneficiaries);

        // LCD - 07
        // Validate inputs
        if ctx.accounts.token_mint.decimals != decimals {
            return Err(VestingError::InvalidDecimals.into());
        }

        if amount > ctx.accounts.wallet_to_withdraw_from.amount {
            return Err(VestingError::InsufficientFunds.into());
        }

        if beneficiaries.len() > 50 {
            return Err(VestingError::TooManyBeneficiaries.into());
        }

        data_account.beneficiaries = beneficiaries;
        data_account.state = 0;
        data_account.token_amount = amount;
        data_account.decimals = decimals; // Because BPF does not support floats
        data_account.initializer = ctx.accounts.sender.key();
        data_account.escrow_wallet = ctx.accounts.escrow_wallet.key();
        data_account.token_mint = ctx.accounts.token_mint.key();
        // LCD - 01
        data_account.initialized_at = Clock::get()?.unix_timestamp as u64;
        data_account.is_initialized = 0; // Mark account as uninitialized

        msg!("Before state: {}", data_account.is_initialized);

        let transfer_instruction = Transfer {
            from: ctx.accounts.wallet_to_withdraw_from.to_account_info(),
            to: ctx.accounts.escrow_wallet.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_instruction
        );

        token::transfer(cpi_ctx, data_account.token_amount * u64::pow(10, decimals as u32))?;
        data_account.is_initialized += 1; // Mark account as initialized

        msg!("After state: {}", data_account.is_initialized);
        msg!("Token transfer completed");

        Ok(())
    }

    // Release function to update the state of the vesting contract
    pub fn release_lucia_vesting(ctx: Context<Release>, _data_bump: u8, state: u8) -> Result<()> {
        let data_account: &mut Account<DataAccount> = &mut ctx.accounts.data_account;

        // 현재 시간 가져오기
        let current_time = Clock::get()?.unix_timestamp;

        // 타임락이 끝나지 않았으면 에러 반환
        if current_time < data_account.time_lock_end {
            msg!("Timelock has not expired yet");
            return Err(VestingError::TimelockNotExpired.into());
        }

        data_account.state = state;

        msg!("Vesting Start - state: {}", state);

        Ok(())
    }

    // Claim function to allow beneficiaries to claim their vested tokens
    pub fn claim_lux(ctx: Context<Claim>, data_bump: u8, _escrow_bump: u8) -> Result<()> {
        let sender = &mut ctx.accounts.sender;
        let escrow_wallet = &mut ctx.accounts.escrow_wallet;
        let data_account = &mut ctx.accounts.data_account;
        let beneficiaries = &data_account.beneficiaries;
        let token_program = &ctx.accounts.token_program;
        let token_mint_key = &ctx.accounts.token_mint.key();
        let beneficiary_ata = &ctx.accounts.wallet_to_deposit_to;
        let decimals = data_account.decimals;
        let state = data_account.state;
        let initialized_at = data_account.initialized_at;

        msg!("Claim Lux!! {:?}", beneficiary_ata);
        msg!("Initialized at: {}", initialized_at);

        // LCD - 03
        if state == 0 {
            return Err(VestingError::ReleaseNotCalled.into());
        }

        let (index, beneficiary) = beneficiaries
            .iter()
            .enumerate()
            .find(|(_, beneficiary)| beneficiary.key == *sender.key)
            .ok_or(VestingError::BeneficiaryNotFound)?;

        let allocated_tokens = beneficiary.allocated_tokens;
        let current_time = Clock::get()?.unix_timestamp;
        let lockup_end_time = (initialized_at as i64) + beneficiary.lockup_period;

        if current_time < lockup_end_time {
            msg!("Lockup period has not expired");
            return Err(VestingError::LockupNotExpired.into());
        }

        let vesting_end_month = beneficiary.vesting_end_month;
        let confirm_round = beneficiary.confirm_round;
        let unlock_tge = beneficiary.unlock_tge;

        // LCD - 02
        let schedule = calculate_schedule(
            lockup_end_time,
            vesting_end_month as i64,
            beneficiary.unlock_duration as i64,
            allocated_tokens as i64,
            unlock_tge,
            confirm_round
        );

        let mut total_claimable_tokens: u64 = 0;

        for item in schedule {
            let round_num = item.0.split(": ").nth(1).unwrap().parse::<u64>().unwrap();

            // Check if the current time is greater than or equal to the unlock time and round_num is valid
            if current_time >= item.1 && (confirm_round as u64) <= round_num {
                msg!(
                    "Tokens claimable: {}, timestamp: {}, claimable_token: {}, first_time_bonus_token: {}",
                    item.0,
                    item.1,
                    item.2,
                    item.3
                );
                // LCD - 09
                // Ensure claimable_token is within the bounds of u64 before adding
                let claimable_token_u64 = if item.2 >= 0.0 && item.2 <= (u64::MAX as f64) {
                    item.2 as u64
                } else {
                    panic!("Invalid claimable_token value");
                };

                // Ensure first_time_bonus is within the bounds of u64 before adding
                let first_time_bonus_u64 = if item.3 >= 0.0 && item.3 <= (u64::MAX as f64) {
                    item.3 as u64
                } else {
                    panic!("Invalid first_time_bonus value");
                };

                total_claimable_tokens = total_claimable_tokens
                    .checked_add(claimable_token_u64)
                    .ok_or(VestingError::Overflow)?;

                total_claimable_tokens = total_claimable_tokens
                    .checked_add(first_time_bonus_u64)
                    .ok_or(VestingError::Overflow)?;
            } else {
                msg!(
                    "Tokens not claimable: {}, timestamp: {}, claimable_token: {}, first_time_bonus_token: {}",
                    item.0,
                    item.1,
                    item.2,
                    item.3
                );
            }

            // LCD - 06
            if vesting_end_month == round_num && current_time > item.1 {
                msg!("Vesting has ended, no more tokens can be claimed.");
            }
        }

        if total_claimable_tokens > 0 {
            msg!("Total claimable tokens: {}", total_claimable_tokens);
        }

        // Ensure total_claimable_tokens is within the bounds of u64 before calculating amount_to_transfer
        let amount_to_transfer = total_claimable_tokens
            .checked_mul(u64::pow(10, decimals as u32))
            .ok_or(VestingError::Overflow)?;

        msg!("Amount to transfer: {}", amount_to_transfer);

        let seeds = &["data_account".as_bytes(), token_mint_key.as_ref(), &[data_bump]];
        let signer_seeds = &[&seeds[..]];

        let transfer_instruction = Transfer {
            from: escrow_wallet.to_account_info(),
            to: beneficiary_ata.to_account_info(),
            authority: data_account.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            token_program.to_account_info(),
            transfer_instruction,
            signer_seeds
        );

        token::transfer(cpi_ctx, amount_to_transfer).map_err(|err| {
            msg!("Token transfer failed: {:?}", err); // Create Err
            ProgramError::Custom(2) // Return to Token transfer failed.
        })?;

        // LCD - 02
        // update confirm_round
        data_account.beneficiaries[index].confirm_round += 1;

        data_account.beneficiaries[index].claimed_tokens += amount_to_transfer;

        msg!("TEST: {}", amount_to_transfer);

        Ok(())
    }
}

// Context for Initialize function
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = sender,
        // LCD - 11
        space = 8 + 1 + 8 + 32 + 32 + 32 + 8 + (4 + 50 * (32 + 8 + 8 + 4 + 8 + 8 + 8 + 1) + 1 + 1), // 3973
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to initialize

    // LCD - 10
    #[account(
        init,
        payer = sender,
        seeds = [b"escrow_wallet", token_mint.key().as_ref()],
        bump,
        token::mint = token_mint,
        token::authority = data_account
    )]
    pub escrow_wallet: Account<'info, TokenAccount>, // Escrow wallet account

    #[account(
        mut,
        constraint = wallet_to_withdraw_from.owner == sender.key(),
        constraint = wallet_to_withdraw_from.mint == token_mint.key(),
    )]
    pub wallet_to_withdraw_from: Account<'info, TokenAccount>, // Account to withdraw tokens from

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub system_program: Program<'info, System>, // System program account

    pub token_program: Program<'info, Token>, // Token program account
}

// Context for Release function
#[derive(Accounts)]
#[instruction(data_bump: u8)]
pub struct Release<'info> {
    #[account(
        mut,
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump = data_bump,
        constraint=data_account.initializer == sender.key() @ VestingError::InvalidSender
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to update

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub system_program: Program<'info, System>, // System program account
}

// Context for Claim function
#[derive(Accounts)]
#[instruction(data_bump: u8, wallet_bump: u8)]
pub struct Claim<'info> {
    #[account(
        mut,
        seeds = [b"data_account", token_mint.key().as_ref()],
        bump = data_bump,
    )]
    pub data_account: Account<'info, DataAccount>, // Data account to update

    #[account(
        mut,
        seeds = [b"escrow_wallet", token_mint.key().as_ref()],
        bump = wallet_bump,
    )]
    pub escrow_wallet: Account<'info, TokenAccount>, // Escrow wallet account

    #[account(mut)]
    pub sender: Signer<'info>, // Signer account

    pub token_mint: Account<'info, Mint>, // Token mint account

    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = token_mint,
        associated_token::authority = sender
    )]
    pub wallet_to_deposit_to: Account<'info, TokenAccount>, // Beneficiary's wallet to deposit to

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

// Struct to represent each beneficiary
#[derive(Default, Copy, Clone, AnchorSerialize, AnchorDeserialize, Debug)]
pub struct Beneficiary {
    pub key: Pubkey, // Beneficiary's public key 32
    pub allocated_tokens: u64, // Tokens allocated to the beneficiary 8
    pub claimed_tokens: u64, // Tokens claimed by the beneficiary 8
    pub unlock_tge: f32, // Unlock percentage at TGE (Token Generation Event) 4
    pub lockup_period: i64, // Lockup period in seconds 8
    pub unlock_duration: u64, // Unlock duration in seconds 8
    pub vesting_end_month: u64, // Vesting end month 8
    pub confirm_round: u8, // Confirmation round 1
}

// Struct to represent the data account
#[account]
#[derive(Default)]
pub struct DataAccount {
    pub state: u8, // State of the vesting contract 1
    pub token_amount: u64, // Total token amount 8
    pub initializer: Pubkey, // Public key of the initializer 32
    pub escrow_wallet: Pubkey, // Public key of the escrow wallet 32
    pub token_mint: Pubkey, // Public key of the token mint 32
    pub initialized_at: u64, // Initialization timestamp 8
    pub beneficiaries: Vec<Beneficiary>, // List of beneficiaries (4 + 50 * (32 + 8 + 8 + 4 + 8 + 8 + 8 + 1)) 3850
    pub decimals: u8, // Token decimals 1
    pub is_initialized: u8, // Flag to check if account is initialized 1
    pub time_lock_end: i64, // Timestamp until which the contract is locked
}

// Enum to represent errors
#[error_code]
pub enum VestingError {
    // Access Control Errors
    #[msg("Sender is not owner of Data Account")]
    InvalidSender,
    #[msg("Unauthorized: Only the contract issuer can initialize the contract.")]
    Unauthorized,

    // Validation Errors
    #[msg("Invalid argument encountered")]
    InvalidArgument,
    #[msg("Invalid token mint.")]
    InvalidTokenMint,
    #[msg("InvalidDecimals: The provided decimals do not match the token mint decimals.")]
    InvalidDecimals,
    #[msg("TooManyBeneficiaries: The number of beneficiaries exceeds the maximum allowed (50).")]
    TooManyBeneficiaries,

    // State Errors
    #[msg("Not allowed to claim new tokens currently")]
    ClaimNotAllowed,
    #[msg("Release function has not been called after initialization.")]
    ReleaseNotCalled,
    #[msg("The program has already been initialized.")]
    AlreadyInitialized,

    // Operational Errors
    #[msg("Beneficiary does not exist in account")]
    BeneficiaryNotFound,
    #[msg("Lockup period has not expired yet.")]
    LockupNotExpired,
    #[msg("InsufficientFunds: The sender's account does not have enough funds.")]
    InsufficientFunds,
    #[msg("Overflow: An overflow occurred during calculations.")]
    Overflow,

    #[msg("Invalid token mint decimals")]
    InvalidDecimalMismatch,

    #[msg("Insufficient token amount")]
    InsufficientTokenAmount,

    #[msg("Timelock has not expired yet.")]
    TimelockNotExpired,
}
