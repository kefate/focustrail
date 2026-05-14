import type { DailyProgress } from "../domain/session";

export function completedRatio(progress: DailyProgress): number {
  const goalSeconds = Math.max(1, progress.dailyGoalMinutes * 60);
  return Math.min(1, Math.max(0, progress.todayCompletedSeconds / goalSeconds));
}

export function remainingMinutes(progress: DailyProgress): number {
  return Math.ceil(progress.todayRemainingSeconds / 60);
}
