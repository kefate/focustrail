use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};

use chrono::Local;
use tauri::AppHandle;

use crate::{domain, storage};

const SYNC_DIR_NAME: &str = "focustrail-records";
const SYNC_SESSIONS_DIR: &str = "sessions";
const SYNC_RECORDS_PATHSPEC: &str = "focustrail-records/sessions";
const SYNC_JSONL_PATHSPEC: &str = ":(glob)focustrail-records/sessions/*.jsonl";
const COMMIT_MESSAGE: &str = "Sync FocusTrail records";
const SYNC_HELPER_ARG: &str = "--focustrail-git-sync";

static SYNC_QUEUE: OnceLock<Mutex<SyncQueue>> = OnceLock::new();

struct GitRepository {
    path: PathBuf,
    remote: String,
    branch: String,
}

struct SyncRequest {
    sessions_dir: PathBuf,
    repo_path: PathBuf,
    log_path: PathBuf,
    commit_body: String,
}

enum SyncOutcome {
    Pushed,
    NoChanges,
    SkippedDirty,
}

#[derive(Default)]
struct SyncQueue {
    running: bool,
    pending: Option<SyncRequest>,
}

pub enum SyncMode {
    Background,
    Detached,
}

pub fn configure_repository(app: &AppHandle) -> Result<Option<storage::Settings>, String> {
    let Some(repo_path) = pick_directory()? else {
        return Ok(None);
    };

    validate_repository(&repo_path)?;

    let mut settings = storage::load_settings(app)?;
    settings.git_sync_repo_path = Some(repo_path.to_string_lossy().into_owned());
    storage::save_settings(app, &settings)?;

    Ok(Some(settings))
}

pub fn schedule_records_sync(
    app: &AppHandle,
    records: &[domain::SessionRecord],
    mode: SyncMode,
) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }

    let settings = storage::load_settings(app)?;
    let Some(repo_path) = settings
        .git_sync_repo_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Ok(());
    };

    let request = SyncRequest {
        sessions_dir: storage::sessions_dir(app)?,
        repo_path: PathBuf::from(repo_path),
        log_path: storage::sync_log_path(app)?,
        commit_body: build_commit_body(records),
    };

    match mode {
        SyncMode::Background => enqueue_sync(request),
        SyncMode::Detached => spawn_sync_helper(request),
    }
}

pub fn run_helper_from_args() -> Result<bool, String> {
    let mut args = std::env::args_os();
    let _exe = args.next();
    let Some(command) = args.next() else {
        return Ok(false);
    };

    if command != OsStr::new(SYNC_HELPER_ARG) {
        return Ok(false);
    }

    let sessions_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "Sync helper is missing the sessions directory.".to_string())?;
    let repo_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "Sync helper is missing the Git repository path.".to_string())?;
    let log_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "Sync helper is missing the sync log path.".to_string())?;
    let commit_body = args
        .next()
        .ok_or_else(|| "Sync helper is missing the commit message.".to_string())?
        .into_string()
        .map_err(|_| "Sync helper commit message is not valid Unicode.".to_string())?;

    run_logged_sync_request(SyncRequest {
        sessions_dir,
        repo_path,
        log_path,
        commit_body,
    })?;
    Ok(true)
}

fn validate_repository(repo_path: &Path) -> Result<GitRepository, String> {
    if !repo_path.exists() {
        return Err("The selected folder does not exist.".to_string());
    }

    if !repo_path.is_dir() {
        return Err("The selected path is not a folder.".to_string());
    }

    let top_level = run_git(repo_path, &["rev-parse", "--show-toplevel"])?;
    let selected_root = canonicalize(repo_path)?;
    let git_root = canonicalize(Path::new(top_level.trim()))?;
    if !same_path(&selected_root, &git_root) {
        return Err("Please choose the Git repository root folder.".to_string());
    }

    let remotes = run_git(repo_path, &["remote"])?;
    let remote = remotes
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "The selected Git repository has no remote configured.".to_string())?
        .to_string();
    run_git(repo_path, &["remote", "get-url", &remote])?;

    let branch = run_git(repo_path, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("The selected Git repository is in detached HEAD state.".to_string());
    }

    Ok(GitRepository {
        path: repo_path.to_path_buf(),
        remote,
        branch: branch.to_string(),
    })
}

fn run_sync_request(request: &SyncRequest) -> Result<SyncOutcome, String> {
    let repo = validate_repository(&request.repo_path)?;
    if has_uncommitted_record_changes(&repo.path)?
        || has_staged_record_changes(&repo.path)?
        || has_untracked_record_files(&repo.path)?
    {
        return Ok(SyncOutcome::SkippedDirty);
    }

    copy_jsonl_records_to_repo(&request.sessions_dir, &repo.path)?;
    commit_and_push(&repo, &request.commit_body)
}

fn run_logged_sync_request(request: SyncRequest) -> Result<SyncOutcome, String> {
    append_sync_log(&request.log_path, "started", "Git sync started.");

    let result = run_sync_request(&request);
    match &result {
        Ok(SyncOutcome::Pushed) => append_sync_log(
            &request.log_path,
            "success",
            "Git sync pushed record changes successfully.",
        ),
        Ok(SyncOutcome::NoChanges) => append_sync_log(
            &request.log_path,
            "success",
            "Git sync completed; no record changes to push.",
        ),
        Ok(SyncOutcome::SkippedDirty) => append_sync_log(
            &request.log_path,
            "skipped",
            "Git sync skipped because focustrail-records/sessions has uncommitted, staged, or untracked changes.",
        ),
        Err(error) => append_sync_log(
            &request.log_path,
            "failed",
            &format!("Git sync failed: {}", sanitize_sync_error(error, &request)),
        ),
    }

    result
}

fn enqueue_sync(request: SyncRequest) -> Result<(), String> {
    let mut queue = sync_queue()
        .lock()
        .map_err(|_| "Git sync queue is unavailable.".to_string())?;
    queue.pending = Some(request);

    if queue.running {
        return Ok(());
    }

    queue.running = true;
    std::thread::spawn(sync_worker);
    Ok(())
}

fn sync_worker() {
    loop {
        let request = {
            let Ok(mut queue) = sync_queue().lock() else {
                return;
            };

            match queue.pending.take() {
                Some(request) => request,
                None => {
                    queue.running = false;
                    return;
                }
            }
        };

        if let Err(error) = run_logged_sync_request(request) {
            eprintln!("FocusTrail Git sync failed: {error}");
        }
    }
}

fn sync_queue() -> &'static Mutex<SyncQueue> {
    SYNC_QUEUE.get_or_init(|| Mutex::new(SyncQueue::default()))
}

fn spawn_sync_helper(request: SyncRequest) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(exe);
    command
        .arg(SYNC_HELPER_ARG)
        .arg(&request.sessions_dir)
        .arg(&request.repo_path)
        .arg(&request.log_path)
        .arg(&request.commit_body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_command_window(&mut command);

    let result = command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start Git sync helper: {error}"));
    if let Err(error) = &result {
        append_sync_log(
            &request.log_path,
            "failed",
            &format!("Git sync failed: {}", sanitize_sync_error(error, &request)),
        );
    }

    result
}

fn copy_jsonl_records_to_repo(source_sessions: &Path, repo_path: &Path) -> Result<(), String> {
    let target_sessions = repo_path.join(SYNC_DIR_NAME).join(SYNC_SESSIONS_DIR);

    fs::create_dir_all(&target_sessions).map_err(|error| error.to_string())?;
    remove_jsonl_files(&target_sessions)?;
    if source_sessions.exists() {
        copy_jsonl_files(source_sessions, &target_sessions)?;
    }

    Ok(())
}

fn commit_and_push(repo: &GitRepository, commit_body: &str) -> Result<SyncOutcome, String> {
    run_git(&repo.path, &["add", "--", SYNC_JSONL_PATHSPEC])?;

    if !has_staged_record_changes(&repo.path)? {
        return Ok(SyncOutcome::NoChanges);
    }

    run_git(
        &repo.path,
        &[
            "commit",
            "-m",
            COMMIT_MESSAGE,
            "-m",
            commit_body,
            "--",
            SYNC_JSONL_PATHSPEC,
        ],
    )?;
    run_git(
        &repo.path,
        &["push", &repo.remote, &format!("HEAD:{}", repo.branch)],
    )?;

    Ok(SyncOutcome::Pushed)
}

fn has_staged_record_changes(repo_path: &Path) -> Result<bool, String> {
    let output = git_command(repo_path)
        .args(["diff", "--cached", "--quiet", "--", SYNC_RECORDS_PATHSPEC])
        .output()
        .map_err(command_start_error)?;

    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(format!(
            "Git command failed: git diff --cached --quiet -- {}{}",
            SYNC_RECORDS_PATHSPEC,
            command_output_suffix(&output.stdout, &output.stderr)
        )),
    }
}

fn has_uncommitted_record_changes(repo_path: &Path) -> Result<bool, String> {
    let output = git_command(repo_path)
        .args(["diff", "--quiet", "--", SYNC_RECORDS_PATHSPEC])
        .output()
        .map_err(command_start_error)?;

    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(format!(
            "Git command failed: git diff --quiet -- {}{}",
            SYNC_RECORDS_PATHSPEC,
            command_output_suffix(&output.stdout, &output.stderr)
        )),
    }
}

fn has_untracked_record_files(repo_path: &Path) -> Result<bool, String> {
    let output = run_git(
        repo_path,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            SYNC_RECORDS_PATHSPEC,
        ],
    )?;
    Ok(!output.trim().is_empty())
}

fn append_sync_log(log_path: &Path, level: &str, message: &str) {
    if let Err(error) = append_sync_log_result(log_path, level, message) {
        eprintln!("FocusTrail Git sync log write failed: {error}");
    }
}

fn append_sync_log_result(log_path: &Path, level: &str, message: &str) -> Result<(), String> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|error| error.to_string())?;
    writeln!(
        file,
        "[{}] {} {}",
        Local::now().format("%Y-%m-%d %H:%M:%S %:z"),
        level,
        message
    )
    .map_err(|error| error.to_string())
}

fn sanitize_sync_error(error: &str, request: &SyncRequest) -> String {
    let mut message = error.to_string();
    for (path, replacement) in [
        (&request.sessions_dir, "<local-sessions>"),
        (&request.repo_path, "<sync-repo>"),
    ] {
        let path = path.to_string_lossy();
        if !path.is_empty() {
            message = message.replace(path.as_ref(), replacement);
        }
    }

    message
}

fn build_commit_body(records: &[domain::SessionRecord]) -> String {
    let mut lines = vec!["FocusTrail session records:".to_string()];
    for record in records {
        lines.push(commit_record_line(record));
    }
    lines.push("Only FocusTrail *.jsonl record files are synchronized.".to_string());
    lines.join("\n")
}

fn commit_record_line(record: &domain::SessionRecord) -> String {
    format!(
        "- {} {}: planned {}, actual {}, {} -> {}",
        time_type_label(record.time_type),
        status_label(record.status),
        minutes_label(record.planned_minutes),
        seconds_label(record.actual_seconds),
        record.started_at.format("%Y-%m-%d %H:%M:%S %:z"),
        record.ended_at.format("%Y-%m-%d %H:%M:%S %:z"),
    )
}

fn time_type_label(time_type: domain::SessionTimeType) -> &'static str {
    match time_type {
        domain::SessionTimeType::Focus => "focus",
        domain::SessionTimeType::Rest => "rest",
    }
}

fn status_label(status: domain::SessionRecordStatus) -> &'static str {
    match status {
        domain::SessionRecordStatus::Completed => "completed",
        domain::SessionRecordStatus::Cancelled => "cancelled",
    }
}

fn minutes_label(minutes: u32) -> String {
    format!("{} min", minutes)
}

fn seconds_label(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        return format!("{} h {} min {} sec", hours, minutes, seconds);
    }

    if minutes > 0 {
        return format!("{} min {} sec", minutes, seconds);
    }

    format!("{} sec", seconds)
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_command(repo_path)
        .args(args)
        .output()
        .map_err(command_start_error)?;

    if !output.status.success() {
        return Err(format!(
            "Git command failed: git {}{}",
            args.join(" "),
            command_output_suffix(&output.stdout, &output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_command(repo_path: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo_path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }
    hide_command_window(&mut command);
    command
}

#[cfg(target_os = "windows")]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_command_window(_command: &mut Command) {}

fn command_start_error(error: std::io::Error) -> String {
    if error.kind() == ErrorKind::NotFound {
        return "Git is not installed or is not available in PATH.".to_string();
    }

    format!("Failed to run Git: {error}")
}

fn command_output_suffix(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let message = stderr.trim();
    let message = if message.is_empty() {
        stdout.trim()
    } else {
        message
    };

    if message.is_empty() {
        String::new()
    } else {
        format!(" ({message})")
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|error| format!("Failed to read folder path: {error}"))
}

#[cfg(windows)]
fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}

fn remove_jsonl_files(target: &Path) -> Result<(), String> {
    for entry in fs::read_dir(target).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_file() && is_jsonl_file(&path) {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn copy_jsonl_files(source: &Path, target: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_file() && is_jsonl_file(&source_path) {
            let target_path = target.join(entry.file_name());
            fs::copy(&source_path, &target_path).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn is_jsonl_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn pick_directory() -> Result<Option<PathBuf>, String> {
    std::thread::spawn(pick_directory_sta)
        .join()
        .map_err(|_| "Folder picker closed unexpectedly.".to_string())?
}

#[cfg(target_os = "windows")]
fn pick_directory_sta() -> Result<Option<PathBuf>, String> {
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
                FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
            },
        },
    };

    const HRESULT_ERROR_CANCELLED: i32 = 0x800704C7u32 as i32;

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| format!("Failed to initialize folder picker: {error}"))?;

        let result = (|| {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("Failed to open folder picker: {error}"))?;
            let title = wide_null("Select Git repository root for FocusTrail sync");

            dialog
                .SetTitle(PCWSTR::from_raw(title.as_ptr()))
                .map_err(|error| format!("Failed to prepare folder picker: {error}"))?;
            dialog
                .SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
                .map_err(|error| format!("Failed to prepare folder picker: {error}"))?;

            if let Err(error) = dialog.Show(None) {
                if error.code().0 == HRESULT_ERROR_CANCELLED {
                    return Ok(None);
                }

                return Err(format!("Failed to show folder picker: {error}"));
            }

            let item = dialog
                .GetResult()
                .map_err(|error| format!("Failed to read selected folder: {error}"))?;
            let path: PWSTR = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|error| format!("Failed to read selected folder: {error}"))?;
            let selected = path.to_string();

            CoTaskMemFree(Some(path.as_ptr() as *const core::ffi::c_void));

            let selected = selected
                .map_err(|error| format!("Selected folder path is not valid Unicode: {error}"))?;

            Ok(Some(PathBuf::from(selected)))
        })();

        CoUninitialize();
        result
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "macos")]
fn pick_directory() -> Result<Option<PathBuf>, String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            "POSIX path of (choose folder with prompt \"Select Git repository root for FocusTrail sync\")",
        ])
        .output()
        .map_err(|error| format!("Failed to open folder picker: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(selected)))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pick_directory() -> Result<Option<PathBuf>, String> {
    let output = Command::new("zenity")
        .args([
            "--file-selection",
            "--directory",
            "--title=Select Git repository root for FocusTrail sync",
        ])
        .output()
        .map_err(|error| format!("Failed to open folder picker: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(selected)))
    }
}
