import { invoke } from "@tauri-apps/api/core";
import type { DailyProgress, RestOverlayMode, Settings, TimerSnapshot } from "../domain/session";

export interface RestOverlayRequest {
  requestId: number;
  visible: boolean;
  durationSeconds: number;
}

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

export function configureGitSyncRepository(): Promise<Settings | null> {
  return invoke("configure_git_sync_repository");
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

export function updateRestOverlayPreferences(
  restOverlayMode: RestOverlayMode,
  restOverlayImage: string | null,
  restOverlayHtml: string | null,
): Promise<Settings> {
  return invoke("update_rest_overlay_preferences", { restOverlayMode, restOverlayImage, restOverlayHtml });
}

export function chooseRestOverlayImage(): Promise<Settings | null> {
  return invoke("choose_rest_overlay_image");
}

export function chooseRestOverlayHtml(): Promise<Settings | null> {
  return invoke("choose_rest_overlay_html");
}

export function readRestOverlayHtml(): Promise<string | null> {
  return invoke("read_rest_overlay_html");
}

export function readRestOverlayImageDataUrl(): Promise<string | null> {
  return invoke("read_rest_overlay_image_data_url");
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

export function showRestOverlay(durationSeconds: number): Promise<RestOverlayRequest> {
  return invoke("show_rest_overlay", { durationSeconds });
}

export function hideRestOverlay(): Promise<void> {
  return invoke("hide_rest_overlay");
}

export function getRestOverlayRequest(): Promise<RestOverlayRequest> {
  return invoke("get_rest_overlay_request");
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
