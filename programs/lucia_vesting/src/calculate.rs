use chrono::{ Duration, Utc, TimeZone };

pub fn calculate_schedule(
    start_time: i64,
    vesting_end_month: i64,
    unlock_duration: i64,
    allocated_tokens: i64,
    confirm_round: u8
) -> Vec<(String, i64, f64)> {
    let mut schedule = Vec::new();
    let start_date = Utc.timestamp_opt(start_time, 0).single().expect("Invalid timestamp");

    let start_round = confirm_round as i64;

    for i in start_round..vesting_end_month + 1 {
        let unlock_time =
            start_date + Duration::seconds((unlock_duration * (i as i64)) / vesting_end_month);

        let claimable_token = (allocated_tokens as f64) / (vesting_end_month as f64);
        let claim_token_round = format!("Round : {}", i);
        let time_round = unlock_time.timestamp();
        let schedule_item = (claim_token_round, time_round, claimable_token as f64);

        schedule.push(schedule_item);
    }
    return schedule;
}
