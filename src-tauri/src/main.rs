#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Mutex,
    thread,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, RunEvent, Size, State, WindowEvent};

mod domain;
mod file_picker;
mod git_sync;
mod stats;
mod storage;

#[derive(Default)]
struct AppState {
    timer: Mutex<domain::TimerState>,
    rest_overlay: Mutex<RestOverlayRuntime>,
}

#[derive(Default)]
struct RestOverlayRuntime {
    request_id: u64,
    visible: bool,
    duration_seconds: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestOverlayRequest {
    request_id: u64,
    visible: bool,
    duration_seconds: u32,
}

const SINGLE_INSTANCE_ADDR: &str = "127.0.0.1:47631";
const REST_OVERLAY_WINDOW_LABEL: &str = "rest-overlay";
const REST_OVERLAY_SHOW_EVENT: &str = "rest-overlay:show";
const DEFAULT_SKIPPED_REST_OVERLAY_SECONDS: u32 = 3 * 60;
const MAX_REST_OVERLAY_SECONDS: u32 = 120 * 60;

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

    append_records(&app, &records, git_sync::SyncMode::Background)?;

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

    append_records(&app, &records, git_sync::SyncMode::Background)?;

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

    append_records(&app, &records, git_sync::SyncMode::Background)?;

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
    let mut settings = storage::load_settings(&app)?;
    settings.daily_goal_minutes = storage::round_goal_to_hours(daily_goal_minutes);
    settings.daily_reset_minutes = storage::normalize_daily_reset_minutes(daily_reset_minutes);
    settings.include_weekends_in_streak = include_weekends_in_streak;
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
    let mut settings = storage::load_settings(&app)?;
    let normalized_focus_minutes = focus_minutes.clamp(1, 720);
    let normalized_rest_minutes = if skip_rest {
        0
    } else {
        rest_minutes.clamp(1, 120)
    };
    settings.focus_minutes = normalized_focus_minutes;
    settings.rest_minutes = normalized_rest_minutes;
    settings.skip_rest = skip_rest;
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
fn update_rest_overlay_preferences(
    app: AppHandle,
    rest_overlay_mode: storage::RestOverlayMode,
    rest_overlay_image: Option<String>,
    rest_overlay_html: Option<String>,
) -> Result<storage::Settings, String> {
    let mut settings = storage::load_settings(&app)?;
    settings.rest_overlay_mode = rest_overlay_mode;
    settings.rest_overlay_image = storage::normalize_optional_text(rest_overlay_image);
    settings.rest_overlay_html = storage::normalize_optional_text(rest_overlay_html);
    storage::save_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn choose_rest_overlay_image(app: AppHandle) -> Result<Option<storage::Settings>, String> {
    save_rest_overlay_asset(
        &app,
        storage::RestOverlayMode::Image,
        file_picker::pick_rest_overlay_image()?,
    )
}

#[tauri::command]
fn choose_rest_overlay_html(app: AppHandle) -> Result<Option<storage::Settings>, String> {
    save_rest_overlay_asset(
        &app,
        storage::RestOverlayMode::Html,
        file_picker::pick_rest_overlay_html()?,
    )
}

#[tauri::command]
fn read_rest_overlay_html(app: AppHandle) -> Result<Option<String>, String> {
    let settings = storage::load_settings(&app)?;
    let Some(html_path) = settings
        .rest_overlay_html
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if html_path.starts_with('<') {
        return Ok(Some(html_path.to_string()));
    }

    let path = std::path::PathBuf::from(html_path);
    if !path.is_file() {
        return Err("The selected HTML file does not exist.".to_string());
    }
    if !rest_overlay_asset_matches(storage::RestOverlayMode::Html, &path) {
        return Err("Please choose an HTML file.".to_string());
    }

    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("Failed to read rest screen HTML: {error}"))
}

#[tauri::command]
fn read_rest_overlay_image_data_url(app: AppHandle) -> Result<Option<String>, String> {
    let settings = storage::load_settings(&app)?;
    let Some(image_path) = settings
        .rest_overlay_image
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if image_path.starts_with("http:")
        || image_path.starts_with("https:")
        || image_path.starts_with("data:")
        || image_path.starts_with("blob:")
    {
        return Ok(Some(image_path.to_string()));
    }

    let path = std::path::PathBuf::from(image_path);
    if !path.is_file() {
        return Err("The selected image file does not exist.".to_string());
    }
    if !rest_overlay_asset_matches(storage::RestOverlayMode::Image, &path) {
        return Err("Please choose an image file.".to_string());
    }

    let mime_type =
        rest_overlay_image_mime(&path).ok_or_else(|| "Please choose an image file.".to_string())?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Failed to read rest screen image: {error}"))?;
    Ok(Some(format!(
        "data:{};base64,{}",
        mime_type,
        base64_encode(&bytes)
    )))
}

#[tauri::command]
fn get_daily_progress(app: AppHandle) -> Result<stats::DailyProgress, String> {
    let settings = storage::load_settings(&app)?;
    let records = storage::read_sessions(&app)?;
    Ok(stats::daily_progress(&records, &settings))
}

#[tauri::command]
fn configure_git_sync_repository(app: AppHandle) -> Result<Option<storage::Settings>, String> {
    git_sync::configure_repository(&app)
}

fn save_rest_overlay_asset(
    app: &AppHandle,
    mode: storage::RestOverlayMode,
    selected: Option<std::path::PathBuf>,
) -> Result<Option<storage::Settings>, String> {
    let Some(path) = selected else {
        return Ok(None);
    };

    if !path.is_file() {
        return Err("Please choose a file.".to_string());
    }

    if !rest_overlay_asset_matches(mode, &path) {
        return Err(match mode {
            storage::RestOverlayMode::Blur => "Please choose a file.".to_string(),
            storage::RestOverlayMode::Image => "Please choose an image file.".to_string(),
            storage::RestOverlayMode::Html => "Please choose an HTML file.".to_string(),
        });
    }

    let mut settings = storage::load_settings(app)?;
    let selected = path.to_string_lossy().into_owned();
    settings.rest_overlay_mode = mode;
    match mode {
        storage::RestOverlayMode::Blur => {}
        storage::RestOverlayMode::Image => settings.rest_overlay_image = Some(selected),
        storage::RestOverlayMode::Html => settings.rest_overlay_html = Some(selected),
    }
    storage::save_settings(app, &settings)?;

    Ok(Some(settings))
}

fn rest_overlay_asset_matches(mode: storage::RestOverlayMode, path: &std::path::Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();

    match mode {
        storage::RestOverlayMode::Blur => true,
        storage::RestOverlayMode::Image => matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg"
        ),
        storage::RestOverlayMode::Html => matches!(extension.as_str(), "html" | "htm"),
    }
}

fn rest_overlay_image_mime(path: &std::path::Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);

        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);

        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }

        if chunk.len() > 2 {
            output.push(TABLE[(third & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }

    output
}

#[tauri::command]
fn show_rest_overlay(
    app: AppHandle,
    duration_seconds: u32,
    state: State<'_, AppState>,
) -> Result<RestOverlayRequest, String> {
    let duration_seconds = normalize_rest_overlay_duration(duration_seconds);
    let request = {
        let mut overlay = state
            .rest_overlay
            .lock()
            .map_err(|_| "Rest overlay state is unavailable".to_string())?;
        overlay.request_id = overlay.request_id.saturating_add(1);
        overlay.visible = true;
        overlay.duration_seconds = duration_seconds;
        RestOverlayRequest {
            request_id: overlay.request_id,
            visible: overlay.visible,
            duration_seconds: overlay.duration_seconds,
        }
    };

    let overlay = app
        .get_webview_window(REST_OVERLAY_WINDOW_LABEL)
        .ok_or_else(|| "Rest overlay window was not found".to_string())?;
    overlay
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    overlay
        .set_fullscreen(true)
        .map_err(|error| error.to_string())?;
    overlay.show().map_err(|error| error.to_string())?;
    overlay.set_focus().map_err(|error| error.to_string())?;
    app.emit_to(
        REST_OVERLAY_WINDOW_LABEL,
        REST_OVERLAY_SHOW_EVENT,
        request.clone(),
    )
    .map_err(|error| error.to_string())?;

    Ok(request)
}

#[tauri::command]
fn hide_rest_overlay(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut overlay = state
            .rest_overlay
            .lock()
            .map_err(|_| "Rest overlay state is unavailable".to_string())?;
        overlay.visible = false;
    }

    if let Some(overlay) = app.get_webview_window(REST_OVERLAY_WINDOW_LABEL) {
        overlay.hide().map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn get_rest_overlay_request(state: State<'_, AppState>) -> Result<RestOverlayRequest, String> {
    let overlay = state
        .rest_overlay
        .lock()
        .map_err(|_| "Rest overlay state is unavailable".to_string())?;
    Ok(RestOverlayRequest {
        request_id: overlay.request_id,
        visible: overlay.visible,
        duration_seconds: overlay.duration_seconds,
    })
}

fn append_records(
    app: &AppHandle,
    records: &[domain::SessionRecord],
    sync_mode: git_sync::SyncMode,
) -> Result<(), String> {
    for record in records {
        storage::append_session(app, record)?;
    }

    if !records.is_empty() {
        git_sync::schedule_records_sync(app, records, sync_mode)?;
    }

    Ok(())
}

fn normalize_rest_overlay_duration(duration_seconds: u32) -> u32 {
    if duration_seconds == 0 {
        return DEFAULT_SKIPPED_REST_OVERLAY_SECONDS;
    }

    duration_seconds.clamp(1, MAX_REST_OVERLAY_SECONDS)
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
    append_records(app, &records, git_sync::SyncMode::Detached)
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
    match git_sync::run_helper_from_args() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("FocusTrail Git sync helper failed: {error}");
            return;
        }
    }

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
            update_rest_overlay_preferences,
            choose_rest_overlay_image,
            choose_rest_overlay_html,
            read_rest_overlay_html,
            read_rest_overlay_image_data_url,
            get_daily_progress,
            configure_git_sync_repository,
            show_rest_overlay,
            hide_rest_overlay,
            get_rest_overlay_request,
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
