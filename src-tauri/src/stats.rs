use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Weekday};
use serde::Serialize;

use crate::{
    domain::{SessionRecord, SessionRecordStatus, SessionTimeType},
    storage::Settings,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyProgress {
    pub daily_goal_minutes: u32,
    pub yesterday_completed_seconds: u64,
    pub today_completed_seconds: u64,
    pub today_rest_seconds: u64,
    pub today_remaining_seconds: u64,
    pub streak_days: u32,
}

pub fn daily_progress(records: &[SessionRecord], settings: &Settings) -> DailyProgress {
    daily_progress_at(records, settings, Local::now())
}

fn daily_progress_at(
    records: &[SessionRecord],
    settings: &Settings,
    now: DateTime<Local>,
) -> DailyProgress {
    let daily_goal_minutes = settings.daily_goal_minutes.clamp(1, 720);
    let daily_reset_minutes = settings.daily_reset_minutes.min(23 * 60 + 59);
    let goal_seconds = u64::from(daily_goal_minutes) * 60;
    let today = reset_period_date(now, daily_reset_minutes);
    let yesterday = today - Duration::days(1);
    let focus_completed_by_date =
        completed_seconds_by_date(records, SessionTimeType::Focus, daily_reset_minutes);
    let rest_completed_by_date =
        completed_seconds_by_date(records, SessionTimeType::Rest, daily_reset_minutes);
    let today_completed_seconds = focus_completed_by_date.get(&today).copied().unwrap_or(0);
    let today_rest_seconds = rest_completed_by_date.get(&today).copied().unwrap_or(0);
    let yesterday_completed_seconds = focus_completed_by_date
        .get(&yesterday)
        .copied()
        .unwrap_or(0);

    DailyProgress {
        daily_goal_minutes,
        yesterday_completed_seconds,
        today_completed_seconds,
        today_rest_seconds,
        today_remaining_seconds: goal_seconds.saturating_sub(today_completed_seconds),
        streak_days: streak_days_from_yesterday(
            &focus_completed_by_date,
            yesterday,
            goal_seconds,
            settings.include_weekends_in_streak,
        ),
    }
}

fn completed_seconds_by_date(
    records: &[SessionRecord],
    time_type: SessionTimeType,
    daily_reset_minutes: u32,
) -> HashMap<NaiveDate, u64> {
    let mut by_date = HashMap::new();

    for record in records {
        if record.status != SessionRecordStatus::Completed {
            continue;
        }
        if record.time_type != time_type {
            continue;
        }

        let date = reset_period_date(record.ended_at, daily_reset_minutes);
        let seconds = record.duration_seconds();
        *by_date.entry(date).or_insert(0) += seconds;
    }

    by_date
}

fn streak_days_from_yesterday(
    completed_by_date: &HashMap<NaiveDate, u64>,
    yesterday: NaiveDate,
    goal_seconds: u64,
    include_weekends: bool,
) -> u32 {
    let mut streak = 0;
    let mut date = yesterday;

    loop {
        if !include_weekends && is_weekend(date) {
            date -= Duration::days(1);
            continue;
        }

        let seconds = completed_by_date.get(&date).copied().unwrap_or(0);
        if seconds < goal_seconds {
            break;
        }

        streak += 1;
        date -= Duration::days(1);
    }

    streak
}

fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn reset_period_date(datetime: DateTime<Local>, daily_reset_minutes: u32) -> NaiveDate {
    let local_minutes = datetime.time().num_seconds_from_midnight() / 60;
    if local_minutes < daily_reset_minutes {
        datetime.date_naive() - Duration::days(1)
    } else {
        datetime.date_naive()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone};

    use super::*;

    fn settings(daily_reset_minutes: u32) -> Settings {
        Settings {
            daily_goal_minutes: 60,
            daily_reset_minutes,
            include_weekends_in_streak: true,
            focus_minutes: 30,
            rest_minutes: 5,
            skip_rest: false,
        }
    }

    fn local_time(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        let local = NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|date| date.and_hms_opt(hour, minute, 0))
            .and_then(|datetime| Local.from_local_datetime(&datetime).earliest())
            .expect("test datetime should be valid in the local timezone");
        local
    }

    fn record(
        ended_at: DateTime<Local>,
        time_type: SessionTimeType,
        actual_seconds: u64,
    ) -> SessionRecord {
        SessionRecord {
            id: "test-record".to_string(),
            session_id: "test-session".to_string(),
            time_type,
            status: SessionRecordStatus::Completed,
            planned_minutes: (actual_seconds / 60) as u32,
            actual_seconds,
            started_at: ended_at - Duration::seconds(actual_seconds as i64),
            ended_at,
        }
    }

    #[test]
    fn uses_previous_period_before_daily_reset_time() {
        let records = vec![
            record(
                local_time(2026, 5, 13, 5, 0),
                SessionTimeType::Focus,
                30 * 60,
            ),
            record(
                local_time(2026, 5, 13, 5, 5),
                SessionTimeType::Rest,
                10 * 60,
            ),
            record(
                local_time(2026, 5, 12, 5, 0),
                SessionTimeType::Focus,
                60 * 60,
            ),
        ];

        let progress =
            daily_progress_at(&records, &settings(4 * 60), local_time(2026, 5, 14, 3, 30));

        assert_eq!(progress.today_completed_seconds, 30 * 60);
        assert_eq!(progress.today_rest_seconds, 10 * 60);
        assert_eq!(progress.yesterday_completed_seconds, 60 * 60);
        assert_eq!(progress.today_remaining_seconds, 30 * 60);
        assert_eq!(progress.streak_days, 1);
    }

    #[test]
    fn starts_new_period_after_daily_reset_time() {
        let records = vec![
            record(
                local_time(2026, 5, 14, 3, 30),
                SessionTimeType::Focus,
                45 * 60,
            ),
            record(
                local_time(2026, 5, 14, 4, 30),
                SessionTimeType::Focus,
                20 * 60,
            ),
            record(
                local_time(2026, 5, 14, 4, 35),
                SessionTimeType::Rest,
                5 * 60,
            ),
        ];

        let progress =
            daily_progress_at(&records, &settings(4 * 60), local_time(2026, 5, 14, 5, 0));

        assert_eq!(progress.today_completed_seconds, 20 * 60);
        assert_eq!(progress.today_rest_seconds, 5 * 60);
        assert_eq!(progress.yesterday_completed_seconds, 45 * 60);
    }
}
