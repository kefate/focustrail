#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Mutex,
    thread,
};

use tauri::{AppHandle, LogicalSize, Manager, RunEvent, Size, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;

mod domain;
mod stats;
mod storage;

#[derive(Default)]
struct AppState {
    timer: Mutex<domain::TimerState>,
}

const SINGLE_INSTANCE_ADDR: &str = "127.0.0.1:47631";

#[tauri::command]
fn get_timer_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    let (snapshot, records) = domain::snapshot_timer(&mut timer);
    drop(timer);

    let should_notify = should_notify_session_completed(&records);
    append_records(&app, &records)?;

    if should_notify {
        notify_session_completed(&app, &records);
    }

    Ok(snapshot)
}

#[tauri::command]
fn start_focus_session(
    planned_minutes: u32,
    rest_minutes: u32,
    state: State<'_, AppState>,
) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    domain::start_timer(&mut timer, planned_minutes, rest_minutes)
}

#[tauri::command]
fn pause_focus_session(state: State<'_, AppState>) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    domain::pause_timer(&mut timer)
}

#[tauri::command]
fn resume_focus_session(state: State<'_, AppState>) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    domain::resume_timer(&mut timer)
}

#[tauri::command]
fn cancel_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    let (snapshot, records) = domain::cancel_timer(&mut timer)?;
    drop(timer);

    append_records(&app, &records)?;

    Ok(snapshot)
}

#[tauri::command]
fn reset_focus_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<domain::TimerSnapshot, String> {
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    let (snapshot, records) = domain::save_and_reset_timer(&mut timer)?;
    drop(timer);

    append_records(&app, &records)?;

    Ok(snapshot)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<storage::Settings, String> {
    storage::load_settings(&app)
}

#[tauri::command]
fn update_daily_goal(
    app: AppHandle,
    daily_goal_minutes: u32,
    daily_reset_minutes: u32,
    include_weekends_in_streak: bool,
) -> Result<storage::Settings, String> {
    let existing = storage::load_settings(&app)?;
    let settings = storage::Settings {
        daily_goal_minutes: storage::round_goal_to_hours(daily_goal_minutes),
        daily_reset_minutes: storage::normalize_daily_reset_minutes(daily_reset_minutes),
        include_weekends_in_streak,
        focus_minutes: existing.focus_minutes,
        rest_minutes: existing.rest_minutes,
        skip_rest: existing.skip_rest,
    };
    storage::save_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn update_focus_preferences(
    app: AppHandle,
    focus_minutes: u32,
    rest_minutes: u32,
    skip_rest: bool,
    state: State<'_, AppState>,
) -> Result<storage::Settings, String> {
    let existing = storage::load_settings(&app)?;
    let normalized_focus_minutes = focus_minutes.clamp(1, 720);
    let normalized_rest_minutes = if skip_rest {
        0
    } else {
        rest_minutes.clamp(1, 120)
    };
    let settings = storage::Settings {
        daily_goal_minutes: existing.daily_goal_minutes,
        daily_reset_minutes: existing.daily_reset_minutes,
        include_weekends_in_streak: existing.include_weekends_in_streak,
        focus_minutes: normalized_focus_minutes,
        rest_minutes: normalized_rest_minutes,
        skip_rest,
    };
    storage::save_settings(&app, &settings)?;
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    domain::apply_idle_preferences(
        &mut timer,
        normalized_focus_minutes,
        normalized_rest_minutes,
    );
    Ok(settings)
}

#[tauri::command]
fn get_daily_progress(app: AppHandle) -> Result<stats::DailyProgress, String> {
    let settings = storage::load_settings(&app)?;
    let records = storage::read_sessions(&app)?;
    Ok(stats::daily_progress(&records, &settings))
}

fn append_records(app: &AppHandle, records: &[domain::SessionRecord]) -> Result<(), String> {
    for record in records {
        storage::append_session(app, record)?;
    }

    Ok(())
}

fn should_notify_session_completed(records: &[domain::SessionRecord]) -> bool {
    records.iter().any(|record| {
        record.status == domain::SessionRecordStatus::Completed
            && record.time_type == domain::SessionTimeType::Focus
    })
}

fn notify_session_completed(app: &AppHandle, records: &[domain::SessionRecord]) {
    let focus_seconds = records
        .iter()
        .filter(|record| {
            record.status == domain::SessionRecordStatus::Completed
                && record.time_type == domain::SessionTimeType::Focus
        })
        .map(|record| record.actual_seconds)
        .sum::<u64>();
    let duration = completed_duration_label(focus_seconds);
    let body = if duration.is_empty() {
        "Nice work. Your focus session is complete.".to_string()
    } else {
        format!("Nice work. You completed {} of focused time.", duration)
    };
    let notification = app
        .notification()
        .builder()
        .title("Focus session complete")
        .body(body);

    #[cfg(not(target_os = "windows"))]
    let notification = if let Some(sound_path) = notification_sound_path() {
        notification.sound(sound_path.to_string_lossy().into_owned())
    } else {
        notification
    };

    let _ = notification.show();
    play_session_completion_sound();
}

fn completed_duration_label(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes > 0 {
        return format!("{} minute{}", minutes, if minutes == 1 { "" } else { "s" });
    }

    if seconds > 0 {
        return format!("{} second{}", seconds, if seconds == 1 { "" } else { "s" });
    }

    String::new()
}

#[cfg(target_os = "windows")]
fn play_session_completion_sound() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{
        PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_NODEFAULT, SND_SYSTEM,
    };

    if let Some(sound_path) = notification_sound_path() {
        let sound_path = sound_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let played = unsafe {
            PlaySoundW(
                sound_path.as_ptr(),
                std::ptr::null_mut(),
                SND_FILENAME | SND_ASYNC | SND_NODEFAULT | SND_SYSTEM,
            )
        };

        if played != 0 {
            return;
        }
    }

    let alias = "SystemNotification"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let _ = unsafe {
        PlaySoundW(
            alias.as_ptr(),
            std::ptr::null_mut(),
            SND_ALIAS | SND_ASYNC | SND_SYSTEM,
        )
    };
}

#[cfg(not(target_os = "windows"))]
fn play_session_completion_sound() {}

fn notification_sound_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var_os("WINDIR")?;
        let sound_path = std::path::PathBuf::from(windir)
            .join("Media")
            .join("Windows Notify System Generic.wav");

        if sound_path.exists() {
            return Some(sound_path);
        }
    }

    None
}

fn persist_active_timer_before_exit(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut timer = state
        .timer
        .lock()
        .map_err(|_| "Timer state is unavailable".to_string())?;
    let (_, mut records) = domain::snapshot_timer(&mut timer);

    if records.is_empty() {
        let (_, reset_records) = domain::save_and_reset_timer(&mut timer)?;
        records = reset_records;
    }

    drop(timer);
    append_records(app, &records)
}

#[tauri::command]
fn show_floating_timer(app: AppHandle) -> Result<(), String> {
    let floating = app
        .get_webview_window("floating")
        .ok_or_else(|| "Floating window was not found".to_string())?;
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window was not found".to_string())?;
    floating
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    floating.show().map_err(|error| error.to_string())?;
    floating.set_focus().map_err(|error| error.to_string())?;
    main.hide().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn hide_floating_timer(app: AppHandle) -> Result<(), String> {
    let floating = app
        .get_webview_window("floating")
        .ok_or_else(|| "Floating window was not found".to_string())?;
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window was not found".to_string())?;
    main.show().map_err(|error| error.to_string())?;
    if main.is_minimized().map_err(|error| error.to_string())? {
        main.unminimize().map_err(|error| error.to_string())?;
    }
    main.set_focus().map_err(|error| error.to_string())?;
    floating.hide().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn focus_main_window(app: AppHandle) -> Result<(), String> {
    focus_main_window_by_handle(&app)
}

fn focus_main_window_by_handle(app: &AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window was not found".to_string())?;
    let floating = app
        .get_webview_window("floating")
        .ok_or_else(|| "Floating window was not found".to_string())?;
    main.show().map_err(|error| error.to_string())?;
    if main.is_minimized().map_err(|error| error.to_string())? {
        main.unminimize().map_err(|error| error.to_string())?;
    }
    main.set_focus().map_err(|error| error.to_string())?;
    floating.hide().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn resize_floating_square(app: AppHandle, logical_size: f64) -> Result<(), String> {
    let floating = app
        .get_webview_window("floating")
        .ok_or_else(|| "Floating window was not found".to_string())?;
    let size = logical_size.clamp(147.0, 720.0);
    floating
        .set_size(Size::Logical(LogicalSize {
            width: size,
            height: size,
        }))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn start_floating_drag(app: AppHandle) -> Result<(), String> {
    let floating = app
        .get_webview_window("floating")
        .ok_or_else(|| "Floating window was not found".to_string())?;
    floating
        .start_dragging()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn claim_single_instance() -> Option<TcpListener> {
    match TcpListener::bind(SINGLE_INSTANCE_ADDR) {
        Ok(listener) => Some(listener),
        Err(_) => {
            let _ = TcpStream::connect(SINGLE_INSTANCE_ADDR)
                .and_then(|mut stream| stream.write_all(b"focus\n"));
            None
        }
    }
}

fn listen_for_second_instances(app: AppHandle, listener: TcpListener) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            if stream.is_err() {
                continue;
            }

            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = focus_main_window_by_handle(&handle);
            });
        }
    });
}

fn main() {
    let Some(single_instance_listener) = claim_single_instance() else {
        return;
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            storage::ensure_data_dirs(&handle)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
            listen_for_second_instances(handle, single_instance_listener);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                let _ = persist_active_timer_before_exit(app);
                app.exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_timer_state,
            start_focus_session,
            pause_focus_session,
            resume_focus_session,
            cancel_focus_session,
            reset_focus_session,
            get_settings,
            update_daily_goal,
            update_focus_preferences,
            get_daily_progress,
            show_floating_timer,
            hide_floating_timer,
            focus_main_window,
            resize_floating_square,
            start_floating_drag
        ])
        .build(tauri::generate_context!())
        .expect("error while building FocusTrail")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                let _ = persist_active_timer_before_exit(app);
            }
        });
}
