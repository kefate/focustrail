import { useEffect, useRef } from "react";
import type { TimerSnapshot } from "../domain/session";
import { showRestOverlay } from "../storage/tauriApi";

const shownSessionStorageKey = "focustrail.restOverlayShownSession";
const skippedRestOverlaySeconds = 3 * 60;

export function useRestOverlayTrigger(timer: TimerSnapshot) {
  const previousTimer = useRef<TimerSnapshot | null>(null);

  useEffect(() => {
    const previous = previousTimer.current;
    previousTimer.current = timer;

    const sessionId = timer.sessionId;
    if (!sessionId) {
      return;
    }

    const focusSeconds = timer.plannedMinutes * 60;
    const focusFinished = focusSeconds > 0 && timer.accumulatedFocusSeconds >= focusSeconds;
    if (!focusFinished) {
      return;
    }

    const enteredRest = timer.status === "running" && timer.phase === "rest" && timer.restMinutes > 0;
    const completedSkippedRest = timer.status === "completed" && timer.phase === "focus" && timer.restMinutes === 0;
    if (!enteredRest && !completedSkippedRest) {
      return;
    }

    const previousAlreadyPastFocus =
      previous?.sessionId === sessionId &&
      previous.accumulatedFocusSeconds >= focusSeconds &&
      (previous.phase === "rest" || previous.status === "completed");
    if (previousAlreadyPastFocus) {
      return;
    }

    const requestKey = `${sessionId}:${timer.restMinutes}`;
    if (window.localStorage.getItem(shownSessionStorageKey) === requestKey) {
      return;
    }

    window.localStorage.setItem(shownSessionStorageKey, requestKey);
    const durationSeconds = timer.restMinutes > 0 ? timer.restMinutes * 60 : skippedRestOverlaySeconds;
    void showRestOverlay(durationSeconds).catch(() => {
      window.localStorage.removeItem(shownSessionStorageKey);
    });
  }, [timer]);
}
