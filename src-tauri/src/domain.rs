use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TimerLifecycleStatus {
    Idle,
    Running,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TimerPhase {
    Focus,
    Rest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionRecordStatus {
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionTimeType {
    Focus,
    Rest,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnapshot {
    pub status: TimerLifecycleStatus,
    pub session_id: Option<String>,
    pub started_at: Option<DateTime<Local>>,
    pub ended_at: Option<DateTime<Local>>,
    pub planned_minutes: u32,
    pub rest_minutes: u32,
    pub phase: TimerPhase,
    pub accumulated_focus_seconds: u64,
    pub accumulated_rest_seconds: u64,
    pub remaining_seconds: u64,
    pub progress: f64,
    pub target_end_at: Option<DateTime<Local>>,
    pub paused_at: Option<DateTime<Local>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: String,
    pub session_id: String,
    #[serde(default = "default_session_time_type")]
    pub time_type: SessionTimeType,
    pub status: SessionRecordStatus,
    pub planned_minutes: u32,
    #[serde(default)]
    pub actual_seconds: u64,
    pub started_at: DateTime<Local>,
    pub ended_at: DateTime<Local>,
}

impl SessionRecord {
    pub fn duration_seconds(&self) -> u64 {
        self.actual_seconds
    }
}

#[derive(Clone, Debug)]
pub struct TimerState {
    pub status: TimerLifecycleStatus,
    pub session_id: Option<String>,
    pub started_at: Option<DateTime<Local>>,
    pub ended_at: Option<DateTime<Local>>,
    pub planned_minutes: u32,
    pub rest_minutes: u32,
    pub phase: TimerPhase,
    pub accumulated_focus_seconds: u64,
    pub accumulated_rest_seconds: u64,
    pub running_started_at: Option<DateTime<Utc>>,
    pub paused_at: Option<DateTime<Local>>,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            status: TimerLifecycleStatus::Idle,
            session_id: None,
            started_at: None,
            ended_at: None,
            planned_minutes: 25,
            rest_minutes: 0,
            phase: TimerPhase::Focus,
            accumulated_focus_seconds: 0,
            accumulated_rest_seconds: 0,
            running_started_at: None,
            paused_at: None,
        }
    }
}

pub fn start_timer(
    timer: &mut TimerState,
    planned_minutes: u32,
    rest_minutes: u32,
) -> Result<TimerSnapshot, String> {
    if matches!(
        timer.status,
        TimerLifecycleStatus::Running | TimerLifecycleStatus::Paused
    ) {
        return Err("A focus session is already active".to_string());
    }

    if planned_minutes == 0 || planned_minutes > 720 {
        return Err("Planned minutes must be between 1 and 720".to_string());
    }

    if rest_minutes > 120 {
        return Err("Rest minutes must be between 0 and 120".to_string());
    }

    Ok(begin_timer(timer, planned_minutes, rest_minutes))
}

pub fn apply_idle_preferences(timer: &mut TimerState, planned_minutes: u32, rest_minutes: u32) {
    if !matches!(
        timer.status,
        TimerLifecycleStatus::Running | TimerLifecycleStatus::Paused
    ) {
        reset_to_idle(timer, planned_minutes.clamp(1, 720), rest_minutes.min(120));
    }
}

pub fn pause_timer(timer: &mut TimerState) -> Result<TimerSnapshot, String> {
    if timer.status != TimerLifecycleStatus::Running {
        return Ok(build_snapshot(timer, Utc::now()));
    }

    let now_utc = Utc::now();
    persist_current_phase_elapsed(timer, now_utc);
    timer.running_started_at = None;
    timer.paused_at = Some(Local::now());
    timer.status = TimerLifecycleStatus::Paused;

    Ok(build_snapshot(timer, now_utc))
}

pub fn resume_timer(timer: &mut TimerState) -> Result<TimerSnapshot, String> {
    if timer.status != TimerLifecycleStatus::Paused {
        return Ok(build_snapshot(timer, Utc::now()));
    }

    let now_utc = Utc::now();
    timer.status = TimerLifecycleStatus::Running;
    timer.running_started_at = Some(now_utc);
    timer.paused_at = None;

    Ok(build_snapshot(timer, now_utc))
}

pub fn cancel_timer(timer: &mut TimerState) -> Result<(TimerSnapshot, Vec<SessionRecord>), String> {
    if !matches!(
        timer.status,
        TimerLifecycleStatus::Running | TimerLifecycleStatus::Paused
    ) {
        return Ok((build_snapshot(timer, Utc::now()), Vec::new()));
    }

    let now_utc = Utc::now();
    persist_current_phase_elapsed(timer, now_utc);
    timer.running_started_at = None;
    timer.paused_at = None;
    timer.ended_at = Some(Local::now());
    timer.status = TimerLifecycleStatus::Cancelled;

    let records = build_records(timer, SessionRecordStatus::Cancelled, now_utc)?;
    Ok((build_snapshot(timer, now_utc), records))
}

pub fn save_and_reset_timer(
    timer: &mut TimerState,
) -> Result<(TimerSnapshot, Vec<SessionRecord>), String> {
    if !matches!(
        timer.status,
        TimerLifecycleStatus::Running | TimerLifecycleStatus::Paused
    ) {
        return Ok((build_snapshot(timer, Utc::now()), Vec::new()));
    }

    let now_utc = Utc::now();
    persist_current_phase_elapsed(timer, now_utc);
    timer.running_started_at = None;
    timer.paused_at = None;
    timer.ended_at = Some(Local::now());

    let planned_minutes = timer.planned_minutes;
    let rest_minutes = timer.rest_minutes;
    let records = if elapsed_focus_seconds(timer, now_utc)
        .saturating_add(elapsed_rest_seconds(timer, now_utc))
        > 0
    {
        build_records(timer, SessionRecordStatus::Completed, now_utc)?
    } else {
        Vec::new()
    };
    reset_to_idle(timer, planned_minutes, rest_minutes);
    let snapshot = build_snapshot(timer, Utc::now());

    Ok((snapshot, records))
}

pub fn snapshot_timer(timer: &mut TimerState) -> (TimerSnapshot, Vec<SessionRecord>) {
    let now_utc = Utc::now();

    if timer.status == TimerLifecycleStatus::Running {
        if let Some(records) = advance_running_timer(timer, now_utc) {
            return (build_snapshot(timer, now_utc), records);
        }
    }

    (build_snapshot(timer, now_utc), Vec::new())
}

fn current_run_seconds(timer: &TimerState, now_utc: DateTime<Utc>) -> u64 {
    let current_run = timer
        .running_started_at
        .map(|started_at| (now_utc - started_at).num_seconds().max(0) as u64)
        .unwrap_or(0);

    current_run
}

fn elapsed_current_phase_seconds(timer: &TimerState, now_utc: DateTime<Utc>) -> u64 {
    let current_run = current_run_seconds(timer, now_utc);
    match timer.phase {
        TimerPhase::Focus => timer.accumulated_focus_seconds.saturating_add(current_run),
        TimerPhase::Rest => timer.accumulated_rest_seconds.saturating_add(current_run),
    }
}

fn elapsed_focus_seconds(timer: &TimerState, now_utc: DateTime<Utc>) -> u64 {
    match timer.phase {
        TimerPhase::Focus => {
            elapsed_current_phase_seconds(timer, now_utc).min(focus_total_seconds(timer))
        }
        TimerPhase::Rest => focus_total_seconds(timer),
    }
}

fn elapsed_rest_seconds(timer: &TimerState, now_utc: DateTime<Utc>) -> u64 {
    match timer.phase {
        TimerPhase::Focus => timer
            .accumulated_rest_seconds
            .min(rest_total_seconds(timer)),
        TimerPhase::Rest => {
            elapsed_current_phase_seconds(timer, now_utc).min(rest_total_seconds(timer))
        }
    }
}

fn persist_current_phase_elapsed(timer: &mut TimerState, now_utc: DateTime<Utc>) {
    let elapsed =
        elapsed_current_phase_seconds(timer, now_utc).min(current_phase_total_seconds(timer));
    match timer.phase {
        TimerPhase::Focus => timer.accumulated_focus_seconds = elapsed,
        TimerPhase::Rest => timer.accumulated_rest_seconds = elapsed,
    }
}

fn advance_running_timer(
    timer: &mut TimerState,
    now_utc: DateTime<Utc>,
) -> Option<Vec<SessionRecord>> {
    loop {
        let phase_total_seconds = current_phase_total_seconds(timer);
        let elapsed = elapsed_current_phase_seconds(timer, now_utc);
        if elapsed < phase_total_seconds {
            return None;
        }

        let overrun_seconds = elapsed.saturating_sub(phase_total_seconds);
        match timer.phase {
            TimerPhase::Focus if rest_total_seconds(timer) > 0 => {
                timer.accumulated_focus_seconds = phase_total_seconds;
                timer.accumulated_rest_seconds = 0;
                timer.phase = TimerPhase::Rest;
                timer.running_started_at = Some(subtract_seconds(now_utc, overrun_seconds));
            }
            TimerPhase::Focus | TimerPhase::Rest => {
                persist_finished_phase(timer);
                timer.running_started_at = None;
                timer.paused_at = None;
                timer.ended_at = Some(Local::now());
                timer.status = TimerLifecycleStatus::Completed;
                return build_records(timer, SessionRecordStatus::Completed, now_utc).ok();
            }
        }
    }
}

fn begin_timer(timer: &mut TimerState, planned_minutes: u32, rest_minutes: u32) -> TimerSnapshot {
    let now_local = Local::now();
    let now_utc = Utc::now();

    timer.status = TimerLifecycleStatus::Running;
    timer.session_id = Some(Uuid::new_v4().to_string());
    timer.started_at = Some(now_local);
    timer.ended_at = None;
    timer.planned_minutes = planned_minutes;
    timer.rest_minutes = rest_minutes;
    timer.phase = TimerPhase::Focus;
    timer.accumulated_focus_seconds = 0;
    timer.accumulated_rest_seconds = 0;
    timer.running_started_at = Some(now_utc);
    timer.paused_at = None;

    build_snapshot(timer, now_utc)
}

fn reset_to_idle(timer: &mut TimerState, planned_minutes: u32, rest_minutes: u32) {
    timer.status = TimerLifecycleStatus::Idle;
    timer.session_id = None;
    timer.started_at = None;
    timer.ended_at = None;
    timer.planned_minutes = planned_minutes;
    timer.rest_minutes = rest_minutes;
    timer.phase = TimerPhase::Focus;
    timer.accumulated_focus_seconds = 0;
    timer.accumulated_rest_seconds = 0;
    timer.running_started_at = None;
    timer.paused_at = None;
}

fn build_snapshot(timer: &TimerState, now_utc: DateTime<Utc>) -> TimerSnapshot {
    let phase_total_seconds = current_phase_total_seconds(timer);
    let phase_elapsed = match timer.status {
        TimerLifecycleStatus::Running => {
            elapsed_current_phase_seconds(timer, now_utc).min(phase_total_seconds)
        }
        _ => elapsed_current_phase_seconds(timer, now_utc).min(phase_total_seconds),
    };
    let remaining_seconds = phase_total_seconds.saturating_sub(phase_elapsed);
    let progress = if phase_total_seconds == 0 {
        0.0
    } else {
        (phase_elapsed as f64 / phase_total_seconds as f64).clamp(0.0, 1.0)
    };
    let target_end_at = if timer.status == TimerLifecycleStatus::Running {
        Some((now_utc + chrono::Duration::seconds(remaining_seconds as i64)).with_timezone(&Local))
    } else {
        None
    };

    TimerSnapshot {
        status: timer.status,
        session_id: timer.session_id.clone(),
        started_at: timer.started_at,
        ended_at: timer.ended_at,
        planned_minutes: timer.planned_minutes,
        rest_minutes: timer.rest_minutes,
        phase: timer.phase,
        accumulated_focus_seconds: elapsed_focus_seconds(timer, now_utc),
        accumulated_rest_seconds: elapsed_rest_seconds(timer, now_utc),
        remaining_seconds,
        progress,
        target_end_at,
        paused_at: timer.paused_at,
    }
}

fn build_records(
    timer: &TimerState,
    status: SessionRecordStatus,
    now_utc: DateTime<Utc>,
) -> Result<Vec<SessionRecord>, String> {
    let session_id = timer
        .session_id
        .clone()
        .ok_or_else(|| "Session id is missing".to_string())?;
    let started_at = timer
        .started_at
        .ok_or_else(|| "Session start time is missing".to_string())?;
    let ended_at = timer
        .ended_at
        .ok_or_else(|| "Session end time is missing".to_string())?;
    let focus_seconds = elapsed_focus_seconds(timer, now_utc);
    let rest_seconds = elapsed_rest_seconds(timer, now_utc);
    let mut records = Vec::new();

    if focus_seconds > 0 {
        records.push(build_phase_record(
            &session_id,
            SessionTimeType::Focus,
            status,
            started_at,
            ended_at,
            timer.planned_minutes,
            focus_seconds,
        ));
    }

    if rest_seconds > 0 {
        records.push(build_phase_record(
            &session_id,
            SessionTimeType::Rest,
            status,
            started_at,
            ended_at,
            timer.planned_minutes,
            rest_seconds,
        ));
    }

    if records.is_empty() {
        records.push(build_phase_record(
            &session_id,
            match timer.phase {
                TimerPhase::Focus => SessionTimeType::Focus,
                TimerPhase::Rest => SessionTimeType::Rest,
            },
            status,
            started_at,
            ended_at,
            timer.planned_minutes,
            0,
        ));
    }

    Ok(records)
}

fn build_phase_record(
    session_id: &str,
    time_type: SessionTimeType,
    status: SessionRecordStatus,
    started_at: DateTime<Local>,
    ended_at: DateTime<Local>,
    planned_minutes: u32,
    actual_seconds: u64,
) -> SessionRecord {
    SessionRecord {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        time_type,
        status,
        planned_minutes,
        actual_seconds,
        started_at,
        ended_at,
    }
}

fn persist_finished_phase(timer: &mut TimerState) {
    match timer.phase {
        TimerPhase::Focus => timer.accumulated_focus_seconds = focus_total_seconds(timer),
        TimerPhase::Rest => timer.accumulated_rest_seconds = rest_total_seconds(timer),
    }
}

fn current_phase_total_seconds(timer: &TimerState) -> u64 {
    match timer.phase {
        TimerPhase::Focus => focus_total_seconds(timer),
        TimerPhase::Rest => rest_total_seconds(timer),
    }
}

fn focus_total_seconds(timer: &TimerState) -> u64 {
    u64::from(timer.planned_minutes) * 60
}

fn rest_total_seconds(timer: &TimerState) -> u64 {
    u64::from(timer.rest_minutes) * 60
}

fn subtract_seconds(now_utc: DateTime<Utc>, seconds: u64) -> DateTime<Utc> {
    now_utc - chrono::Duration::seconds(seconds.min(i64::MAX as u64) as i64)
}

fn default_session_time_type() -> SessionTimeType {
    SessionTimeType::Focus
}
