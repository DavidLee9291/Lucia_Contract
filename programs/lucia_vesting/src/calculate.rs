use chrono::{Duration, TimeZone, Utc};

// LCD-02
pub fn calculate_schedule(
    start_time: i64,
    vesting_end_month: i64,
    unlock_duration: i64,
    allocated_tokens: i64,
    confirm_round: u8,
) -> Vec<(String, i64, f64)> {
    let mut schedule = Vec::new();
    // LCD - 02
    let start_date = Utc
        .timestamp_opt(start_time, 0)
        .single()
        .expect("Invalid timestamp");

    let start_round = confirm_round as i64;

    // LCD - 09
    // Ensure allocated_tokens and vesting_end_month are positive to avoid unexpected behavior
    if allocated_tokens < 0 || vesting_end_month <= 0 {
        panic!("Invalid allocated_tokens or vesting_end_month");
    }
    // Check for overflow before casting
    let claimable_token = (allocated_tokens as f64) / (vesting_end_month as f64);
    if claimable_token < 0.0 || claimable_token.is_infinite() || claimable_token.is_nan() {
        panic!("Invalid claimable_token calculated");
    }

    for i in start_round..vesting_end_month + 1 {
        let unlock_time =
            start_date + Duration::seconds((unlock_duration * (i as i64)) / vesting_end_month);

        let claim_token_round = format!("Round : {}", i);
        let time_round = unlock_time.timestamp();
        let schedule_item = (claim_token_round, time_round, claimable_token as f64);

        schedule.push(schedule_item);
    }
    return schedule;
}