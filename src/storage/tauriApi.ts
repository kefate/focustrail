import { invoke } from "@tauri-apps/api/core";
import type { DailyProgress, Settings, TimerSnapshot } from "../domain/session";

export function getTimerState(): Promise<TimerSnapshot> {
  return invoke("get_timer_state");
}

export function startFocusSession(plannedMinutes: number, restMinutes: number): Promise<TimerSnapshot> {
  return invoke("start_focus_session", { plannedMinutes, restMinutes });
}

export function pauseFocusSession(): Promise<TimerSnapshot> {
  return invoke("pause_focus_session");
}

export function resumeFocusSession(): Promise<TimerSnapshot> {
  return invoke("resume_focus_session");
}

export function cancelFocusSession(): Promise<TimerSnapshot> {
  return invoke("cancel_focus_session");
}

export function resetFocusSession(): Promise<TimerSnapshot> {
  return invoke("reset_focus_session");
}

export function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export function updateDailyGoal(
  dailyGoalMinutes: number,
  dailyResetMinutes: number,
  includeWeekendsInStreak: boolean,
): Promise<Settings> {
  return invoke("update_daily_goal", { dailyGoalMinutes, dailyResetMinutes, includeWeekendsInStreak });
}

export function updateFocusPreferences(focusMinutes: number, restMinutes: number, skipRest: boolean): Promise<Settings> {
  return invoke("update_focus_preferences", { focusMinutes, restMinutes, skipRest });
}

export function getDailyProgress(): Promise<DailyProgress> {
  return invoke("get_daily_progress");
}

export function showFloatingTimer(): Promise<void> {
  return invoke("show_floating_timer");
}

export function hideFloatingTimer(): Promise<void> {
  return invoke("hide_floating_timer");
}

export function focusMainWindow(): Promise<void> {
  return invoke("focus_main_window");
}

export function resizeFloatingSquare(logicalSize: number): Promise<void> {
  return invoke("resize_floating_square", { logicalSize });
}

export function startFloatingDrag(): Promise<void> {
  return invoke("start_floating_drag");
}
