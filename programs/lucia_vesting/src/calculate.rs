// LCD-02
pub fn calculate_schedule(
    start_time: i64,
    vesting_end_month: i64,
    unlock_duration: i64,
    allocated_tokens: i64,
    unlock_tge: f32,
    confirm_round: u8
) -> Vec<(String, i64, f64, f64)> {
    let mut schedule = Vec::new();
    let start_round = confirm_round as i64;

    // Calculate first_time_bonus if unlock_tge is greater than 0.0
    let mut remaining_tokens = allocated_tokens;
    let first_time_bonus = if unlock_tge > 0.0 {
        let bonus = allocated_tokens / (unlock_tge as i64);
        remaining_tokens -= bonus;
        bonus
    } else {
        0
    };

    // Calculate claimable_token per month excluding the first_time_bonus
    let claimable_token = (remaining_tokens as f64) / (vesting_end_month as f64);

    // Ensure remaining_tokens and vesting_end_month are positive to avoid unexpected behavior
    if remaining_tokens < 0 || vesting_end_month <= 0 {
        panic!("Invalid remaining_tokens or vesting_end_month");
    }

    // Check for overflow before casting
    if claimable_token < 0.0 || claimable_token.is_infinite() || claimable_token.is_nan() {
        panic!("Invalid claimable_token calculated");
    }

    for i in start_round..=vesting_end_month {
        // Calculate unlock_time for each round
        let unlock_time = start_time + (unlock_duration * (i as i64)) / vesting_end_month;

        // Add first_time_bonus only for the first month
        let total_claimable = if i == start_round && unlock_tge != 0.0 {
            claimable_token + (first_time_bonus as f64)
        } else {
            claimable_token
        };

        // Create schedule item and push to schedule vector
        let claim_token_round = format!("Round : {}", i);
        let schedule_item = (
            claim_token_round,
            unlock_time,
            total_claimable,
            if i == start_round { first_time_bonus as f64 } else { 0.0 },
        );
        schedule.push(schedule_item);
    }

    schedule
}
