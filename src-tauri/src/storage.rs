use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::domain::SessionRecord;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub daily_goal_minutes: u32,
    #[serde(default)]
    pub daily_reset_minutes: u32,
    #[serde(default = "default_include_weekends_in_streak")]
    pub include_weekends_in_streak: bool,
    #[serde(default = "default_focus_minutes")]
    pub focus_minutes: u32,
    #[serde(default = "default_rest_minutes")]
    pub rest_minutes: u32,
    #[serde(default)]
    pub skip_rest: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            daily_goal_minutes: 240,
            daily_reset_minutes: 0,
            include_weekends_in_streak: true,
            focus_minutes: 30,
            rest_minutes: 5,
            skip_rest: false,
        }
    }
}

pub fn ensure_data_dirs(app: &AppHandle) -> Result<(), String> {
    fs::create_dir_all(sessions_dir(app)?).map_err(|error| error.to_string())?;
    let settings_path = settings_path(app)?;
    if !settings_path.exists() {
        save_settings(app, &Settings::default())?;
    }
    Ok(())
}

pub fn load_settings(app: &AppHandle) -> Result<Settings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        let settings = Settings::default();
        save_settings(app, &settings)?;
        return Ok(settings);
    }

    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut settings: Settings =
        serde_json::from_reader(file).map_err(|error| error.to_string())?;
    settings.daily_goal_minutes = round_goal_to_hours(settings.daily_goal_minutes).clamp(60, 720);
    settings.daily_reset_minutes = normalize_daily_reset_minutes(settings.daily_reset_minutes);
    settings.focus_minutes = settings.focus_minutes.clamp(1, 720);
    settings.rest_minutes = if settings.skip_rest {
        0
    } else {
        settings.rest_minutes.clamp(1, 120)
    };
    Ok(settings)
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = File::create(path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(file, settings).map_err(|error| error.to_string())
}

pub fn append_session(app: &AppHandle, record: &SessionRecord) -> Result<(), String> {
    fs::create_dir_all(sessions_dir(app)?).map_err(|error| error.to_string())?;
    let path = session_file_path(app, record)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut file, record).map_err(|error| error.to_string())?;
    writeln!(file).map_err(|error| error.to_string())
}

pub fn read_sessions(app: &AppHandle) -> Result<Vec<SessionRecord>, String> {
    let dir = sessions_dir(app)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let is_jsonl = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false);

        if !is_jsonl {
            continue;
        }

        let file = File::open(path).map_err(|error| error.to_string())?;
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|error| error.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<SessionRecord>(&line) {
                records.push(record);
            }
        }
    }

    Ok(records)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("data"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("settings.json"))
}

fn sessions_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("sessions"))
}

fn session_file_path(app: &AppHandle, record: &SessionRecord) -> Result<PathBuf, String> {
    let file_name = format!("{}.jsonl", record.started_at.format("%Y-%m"));
    Ok(sessions_dir(app)?.join(file_name))
}

fn default_include_weekends_in_streak() -> bool {
    true
}

fn default_focus_minutes() -> u32 {
    30
}

fn default_rest_minutes() -> u32 {
    5
}

pub fn round_goal_to_hours(minutes: u32) -> u32 {
    let hours = ((minutes + 30) / 60).clamp(1, 12);
    hours * 60
}

pub fn normalize_daily_reset_minutes(minutes: u32) -> u32 {
    minutes.min(23 * 60 + 59)
}
