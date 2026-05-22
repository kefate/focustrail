# FocusTrail

FocusTrail is a Windows desktop focus timer MVP built with Tauri 2, React, and TypeScript. Latest version provides a focused-session experience inspired by the Windows Clock focus sessions: a main timer window, an always-on-top floating timer, append-only local JSONL session logs, optional Git-backed record sync, daily progress stats, and Windows completion notifications.

## Getting Started

```bash
npm install
npm run tauri:dev
```

To check the frontend build only:

```bash
npm run build
```

To build a debug desktop executable:

```bash
npm run tauri -- build --debug
```

## Data Location

FocusTrail stores data through the Tauri backend under the app data directory. On Windows, the default location is similar to:

```text
%APPDATA%\com.focustrail.desktop\data
```

The data directory is organized as:

```text
data/
  settings.json
  focustrail-sync.log
  sessions/
    2026-05.jsonl
```

`sessions` uses append-only JSONL. Each focus or break segment is stored as one JSON line and is distinguished by `timeType` (`focus` or `rest`). Each record has a globally unique `id`; records from the same focus session share the same `sessionId`.

Only records with `status: "completed"` and `timeType: "focus"` count toward the daily goal, yesterday's completed time, and streaks. Break records are shown separately as today's rested time. Cancelled records are kept in the log but do not count toward progress. Daily progress does not delete historical logs; it uses the daily reset time in `settings.json` to determine the current reporting day.

## Git Sync

FocusTrail can sync local session records into a user-selected Git repository. Use the settings button in the lower-left corner of the main window to choose a folder. The selected folder must be the root of a Git repository, have a remote configured, and have an upstream branch that can be pushed.

After local records are saved, FocusTrail copies monthly `*.jsonl` session logs into `focustrail-records/sessions/` inside the configured repository, commits the copied record changes, and pushes them to the remote branch. Commit messages include the focus or rest type, status, planned duration, actual duration, and start/end times for the recorded segment, without including local computer paths.

Sync work runs in the background so timer reset, window closing, and other app actions can continue. FocusTrail only stages `focustrail-records/sessions/*.jsonl`; if that sync path already has uncommitted, staged, or untracked changes, the sync is skipped to avoid mixing manual edits into an automatic commit. Each sync attempt appends its result to `focustrail-sync.log`.

## Features

- Main window with focus-session and daily-progress cards.
- Focus timer states: `idle`, `running`, `paused`, `completed`, and `cancelled`.
- Configurable focus duration, optional break duration, start/pause/resume/cancel, and save-and-reset.
- Floating timer window that stays on top, keeps a square resize ratio, can pause/resume, and can return to the main window.
- Mutually exclusive main and floating windows.
- Windows toast notification with sound when a focus session naturally completes.
- Daily progress with daily goal, yesterday's completed focus time, today's completed focus time, remaining focus time, today's rested time, and streak count from yesterday.
- Daily goal editor with hour-based goals, daily reset time, and weekend inclusion for streaks.
- Local file storage with `settings.json` and monthly JSONL session logs.
- Optional Git sync for JSONL session logs.
- Single-instance behavior: launching the app again focuses the existing main window.

## Not Supported Yet

FocusTrail does not include login, GitHub OAuth, task management, project management, cloud sync, advanced reports, calendar views, or white noise.

## Development Notes

- Local session logs are intentionally append-only.
- Git sync is limited to copied JSONL session logs under `focustrail-records/sessions/`.
- Background sync results are appended to `focustrail-sync.log` in the app data directory.
- The debug executable is generated under `src-tauri/target/debug/focustrail.exe`.

## Sample

### Main Window

<img src="docs/screenshots/main-window.png" width="600" alt="Main Window">

### Floating Timer

<img src="docs/screenshots/floating-timer.png" width="220" alt="Floating Timer">
