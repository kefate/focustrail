import { useCallback, useEffect, useRef, useState } from "react";
import type { TimerSnapshot } from "../domain/session";
import { getTimerState } from "../storage/tauriApi";

const idleSnapshot: TimerSnapshot = {
  status: "idle",
  sessionId: null,
  startedAt: null,
  endedAt: null,
  plannedMinutes: 25,
  restMinutes: 0,
  phase: "focus",
  accumulatedFocusSeconds: 0,
  accumulatedRestSeconds: 0,
  remainingSeconds: 25 * 60,
  progress: 0,
  targetEndAt: null,
  pausedAt: null,
};

export function useTimerState(pollMs: number) {
  const [timer, setTimer] = useState<TimerSnapshot>(idleSnapshot);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await getTimerState();
      if (alive.current) {
        setTimer(next);
        setError(null);
      }
    } catch (reason) {
      if (alive.current) {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    const id = window.setInterval(() => {
      void refresh();
    }, pollMs);

    return () => {
      alive.current = false;
      window.clearInterval(id);
    };
  }, [pollMs, refresh]);

  return { timer, error, refresh, setTimer };
}
