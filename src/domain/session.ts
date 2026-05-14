export type TimerStatus = "idle" | "running" | "paused" | "completed" | "cancelled";
export type TimerPhase = "focus" | "rest";

export interface TimerSnapshot {
  status: TimerStatus;
  sessionId: string | null;
  startedAt: string | null;
  endedAt: string | null;
  plannedMinutes: number;
  restMinutes: number;
  phase: TimerPhase;
  accumulatedFocusSeconds: number;
  accumulatedRestSeconds: number;
  remainingSeconds: number;
  progress: number;
  targetEndAt: string | null;
  pausedAt: string | null;
}

export interface Settings {
  dailyGoalMinutes: number;
  dailyResetMinutes: number;
  includeWeekendsInStreak: boolean;
  focusMinutes: number;
  restMinutes: number;
  skipRest: boolean;
}

export interface DailyProgress {
  dailyGoalMinutes: number;
  yesterdayCompletedSeconds: number;
  todayCompletedSeconds: number;
  todayRestSeconds: number;
  todayRemainingSeconds: number;
  streakDays: number;
}

export const statusLabel: Record<TimerStatus, string> = {
  idle: "idle",
  running: "running",
  paused: "paused",
  completed: "completed",
  cancelled: "cancelled",
};

export function formatClock(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${rest.toString().padStart(2, "0")}`;
  }

  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}

export function formatHours(seconds: number): string {
  const hours = seconds / 3600;
  return `${hours.toFixed(hours >= 10 ? 1 : 2)} h`;
}

export function formatWholeHours(seconds: number): string {
  return `${Math.floor(seconds / 3600)} h`;
}

export function formatHoursMinutes(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  return `${hours} h, ${minutes} min`;
}

export function splitHoursMinutes(seconds: number): { hours: string; minutes: string } {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  return {
    hours: String(Math.floor(safeSeconds / 3600)),
    minutes: String(Math.floor((safeSeconds % 3600) / 60)),
  };
}

export function minuteLabel(seconds: number): { value: string; unit: string } {
  const minutes = Math.max(0, Math.ceil(seconds / 60));
  if (minutes >= 60 && minutes % 60 === 0) {
    return { value: String(minutes / 60), unit: "h" };
  }

  return { value: String(minutes), unit: "min" };
}

export function clampMinutes(value: number): number {
  if (!Number.isFinite(value)) {
    return 25;
  }

  return Math.min(720, Math.max(1, Math.round(value)));
}
