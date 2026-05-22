import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import {
  focusMainWindow,
  getSettings,
  pauseFocusSession,
  resetFocusSession,
  resizeFloatingSquare,
  resumeFocusSession,
  startFocusSession,
  startFloatingDrag,
} from "../storage/tauriApi";
import type { Settings } from "../domain/session";
import { TimerDial } from "./TimerDial";
import { useTimerState } from "./useTimerState";

const floatingMinSize = 147;

export function FloatingTimer() {
  const { timer, setTimer } = useTimerState(300);
  const [settings, setSettings] = useState<Settings>({
    dailyGoalMinutes: 240,
    dailyResetMinutes: 0,
    includeWeekendsInStreak: true,
    focusMinutes: 30,
    restMinutes: 5,
    skipRest: false,
    gitSyncRepoPath: null,
  });
  const [menuOpen, setMenuOpen] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(false);
  const previousSquareSize = useRef(296);

  const refreshSettings = useCallback(async () => {
    const next = await getSettings();
    setSettings(next);
  }, []);

  useEffect(() => {
    let timeout = 0;

    function enforceSquare() {
      window.clearTimeout(timeout);
      timeout = window.setTimeout(() => {
        const width = Math.round(window.innerWidth);
        const height = Math.round(window.innerHeight);
        if (Math.abs(width - height) <= 2) {
          previousSquareSize.current = Math.max(width, height);
          return;
        }

        const nextSize = Math.min(width, height) < previousSquareSize.current ? Math.min(width, height) : Math.max(width, height);
        previousSquareSize.current = nextSize;
        void resizeFloatingSquare(nextSize).catch(() => undefined);
      }, 140);
    }

    enforceSquare();
    window.addEventListener("resize", enforceSquare);
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener("resize", enforceSquare);
    };
  }, []);

  useEffect(() => {
    let alive = true;

    async function refreshIfAlive() {
      try {
        const next = await getSettings();
        if (alive) {
          setSettings(next);
        }
      } catch {
        // Keep the last settings snapshot if the backend is not ready yet.
      }
    }

    void refreshIfAlive();
    const id = window.setInterval(() => {
      void refreshIfAlive();
    }, 1000);

    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  useEffect(() => {
    function refreshOnFocus() {
      void refreshSettings().catch(() => undefined);
    }

    window.addEventListener("focus", refreshOnFocus);
    return () => window.removeEventListener("focus", refreshOnFocus);
  }, [refreshSettings]);

  useEffect(() => {
    function hideControls() {
      setControlsVisible(false);
      setMenuOpen(false);
    }

    window.addEventListener("blur", hideControls);
    return () => window.removeEventListener("blur", hideControls);
  }, []);

  async function togglePause() {
    if (timer.status !== "running" && timer.status !== "paused") {
      const latest = await getSettings();
      setSettings(latest);
      const next = await startFocusSession(latest.focusMinutes, latest.skipRest ? 0 : latest.restMinutes);
      setTimer(next);
      return;
    }

    const next = timer.status === "running" ? await pauseFocusSession() : await resumeFocusSession();
    setTimer(next);
  }

  async function resetSession() {
    if (timer.status !== "paused") {
      return;
    }

    const next = await resetFocusSession();
    setTimer(next);
    setMenuOpen(false);
  }

  function returnToMain() {
    void focusMainWindow();
  }

  function shrinkToMinimum() {
    previousSquareSize.current = floatingMinSize;
    setMenuOpen(false);
    void resizeFloatingSquare(floatingMinSize).catch(() => undefined);
  }

  function startDragging(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0) {
      return;
    }

    const target = event.target as HTMLElement;
    if (target.closest("button,input,select,textarea")) {
      return;
    }

    void startFloatingDrag().catch(() => undefined);
  }

  const displayTimer =
    timer.status === "running" || timer.status === "paused"
      ? timer
      : {
          ...timer,
          phase: "focus" as const,
          plannedMinutes: settings.focusMinutes,
          restMinutes: settings.skipRest ? 0 : settings.restMinutes,
          accumulatedFocusSeconds: 0,
          accumulatedRestSeconds: 0,
          remainingSeconds: settings.focusMinutes * 60,
          progress: 0,
        };

  return (
    <main
      className={controlsVisible || menuOpen ? "floating-shell controls-visible" : "floating-shell"}
      onMouseDown={startDragging}
      onMouseEnter={() => {
        setControlsVisible(true);
        void refreshSettings().catch(() => undefined);
      }}
      onFocusCapture={() => {
        setControlsVisible(true);
        void refreshSettings().catch(() => undefined);
      }}
      onMouseLeave={() => {
        setControlsVisible(false);
        setMenuOpen(false);
      }}
    >
      <header className="floating-titlebar" data-tauri-drag-region>
        <button onClick={returnToMain} aria-label="Return to main window" title="Return to main window">
          ⛶
        </button>
        <span>Focus session</span>
        <button onClick={shrinkToMinimum} aria-label="Shrink to minimum" title="Shrink to minimum">
          −
        </button>
      </header>

      <section className="floating-content">
        <TimerDial timer={displayTimer} className="floating-dial" onClick={() => void togglePause()} />

        <div className="floating-actions">
          <button
            className="round-primary"
            onClick={() => void togglePause()}
            aria-label={timer.status === "running" ? "Pause" : timer.status === "paused" ? "Resume" : "Start"}
          >
            <FloatingActionIcon kind={timer.status === "running" ? "pause" : "play"} />
          </button>
          {timer.status === "paused" && (
            <button className="round-secondary reset-button" onClick={() => void resetSession()} aria-label="Save and reset" title="Save and reset">
              <FloatingActionIcon kind="reset" />
            </button>
          )}
          <button className="round-secondary" onClick={() => setMenuOpen((open) => !open)} aria-label="More">
            <DotsIcon />
          </button>
        </div>

        {menuOpen && <div className="floating-menu">Click the timer or blue button to pause or resume.</div>}

        <p className="next-break">
          {displayTimer.phase === "rest" ? (
            <strong>Resting</strong>
          ) : settings.skipRest ? (
            <strong>Next: no break</strong>
          ) : (
            <>
              Next: <strong>{settings.restMinutes} min break</strong>
            </>
          )}
        </p>
      </section>
    </main>
  );
}

type FloatingActionIconKind = "pause" | "play" | "reset";

interface FloatingActionIconProps {
  kind: FloatingActionIconKind;
}

function FloatingActionIcon({ kind }: FloatingActionIconProps) {
  if (kind === "pause") {
    return (
      <span className="pause-icon" aria-hidden="true">
        <span />
        <span />
      </span>
    );
  }

  if (kind === "reset") {
    return (
      <span className="reset-icon" aria-hidden="true">
        ↻
      </span>
    );
  }

  return <span className="play-icon" aria-hidden="true" />;
}

function DotsIcon() {
  return (
    <span className="dots-icon" aria-hidden="true">
      <span />
      <span />
      <span />
    </span>
  );
}
