import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { clampMinutes, formatHoursMinutes, splitHoursMinutes } from "../domain/session";
import type { DailyProgress, Settings, TimerSnapshot } from "../domain/session";
import { completedRatio } from "../stats/progress";
import {
  cancelFocusSession,
  configureGitSyncRepository,
  getDailyProgress,
  getSettings,
  pauseFocusSession,
  resetFocusSession,
  resumeFocusSession,
  showFloatingTimer,
  startFocusSession,
  updateDailyGoal,
  updateFocusPreferences,
} from "../storage/tauriApi";
import { TimerDial } from "./TimerDial";
import { useTimerState } from "./useTimerState";

const defaultSettings: Settings = {
  dailyGoalMinutes: 240,
  dailyResetMinutes: 0,
  includeWeekendsInStreak: true,
  focusMinutes: 30,
  restMinutes: 5,
  skipRest: false,
  gitSyncRepoPath: null,
};

export function MainWindow() {
  const { timer, error, setTimer } = useTimerState(500);
  const [selectedMinutes, setSelectedMinutes] = useState(30);
  const [customMinutes, setCustomMinutes] = useState(30);
  const [restMinutes, setRestMinutes] = useState(5);
  const [skipRest, setSkipRest] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [editingGoal, setEditingGoal] = useState(false);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [progress, setProgress] = useState<DailyProgress | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [syncNotice, setSyncNotice] = useState<string | null>(null);
  const [syncBusy, setSyncBusy] = useState(false);

  const plannedMinutes = useMemo(() => clampMinutes(selectedMinutes), [selectedMinutes]);

  async function runAction(action: () => Promise<TimerSnapshot>, refreshProgress = false) {
    try {
      const next = await action();
      setTimer(next);
      setActionError(null);
      if (next.status === "idle" || next.status === "completed" || next.status === "cancelled") {
        setMenuOpen(false);
      }
      if (refreshProgress || next.status === "completed" || next.status === "cancelled") {
        await loadProgress();
      }
    } catch (reason) {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function loadProgress() {
    const [nextSettings, nextProgress] = await Promise.all([getSettings(), getDailyProgress()]);
    setSettings(nextSettings);
    setProgress(nextProgress);
    setSelectedMinutes(nextSettings.focusMinutes);
    setCustomMinutes(nextSettings.focusMinutes);
    setRestMinutes(nextSettings.restMinutes);
    setSkipRest(nextSettings.skipRest);
  }

  useEffect(() => {
    void loadProgress().catch((reason) => {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    });
  }, []);

  useEffect(() => {
    if (timer.status === "completed" || timer.status === "cancelled") {
      void loadProgress().catch((reason) => {
        setActionError(reason instanceof Error ? reason.message : String(reason));
      });
    }
  }, [timer.status]);

  useEffect(() => {
    if (!syncNotice) {
      return;
    }

    const id = window.setTimeout(() => setSyncNotice(null), 3600);
    return () => window.clearTimeout(id);
  }, [syncNotice]);

  useEffect(() => {
    if (!menuOpen) {
      return;
    }

    function closeMenuOnOutsideClick(event: PointerEvent) {
      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }

      if (target.closest("[data-focus-menu]") || target.closest("[data-focus-menu-toggle]")) {
        return;
      }

      setMenuOpen(false);
    }

    window.addEventListener("pointerdown", closeMenuOnOutsideClick);
    return () => window.removeEventListener("pointerdown", closeMenuOnOutsideClick);
  }, [menuOpen]);

  async function handleSwitchToFloating() {
    try {
      await persistFocusPreferences(plannedMinutes, restMinutes, skipRest);
      await showFloatingTimer();
    } catch (reason) {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function persistFocusPreferences(focusMinutes: number, nextRestMinutes: number, nextSkipRest: boolean) {
    const next = await updateFocusPreferences(focusMinutes, nextRestMinutes, nextSkipRest);
    setSettings(next);
  }

  function handleFocusMinutesChange(value: number) {
    const next = clampMinutes(value);
    setCustomMinutes(next);
    setSelectedMinutes(next);
    void persistFocusPreferences(next, restMinutes, skipRest).catch((reason) => {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    });
  }

  function handleRestMinutesChange(value: number) {
    const next = Math.min(120, clampMinutes(value));
    setRestMinutes(next);
    void persistFocusPreferences(plannedMinutes, next, skipRest).catch((reason) => {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    });
  }

  function handleSkipRestChange(next: boolean) {
    const nextRestMinutes = next ? 0 : 5;
    setSkipRest(next);
    setRestMinutes(nextRestMinutes);
    void persistFocusPreferences(plannedMinutes, nextRestMinutes, next).catch((reason) => {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    });
  }

  async function handleSaveGoal(dailyGoalMinutes: number, dailyResetMinutes: number, includeWeekendsInStreak: boolean) {
    try {
      const next = await updateDailyGoal(dailyGoalMinutes, dailyResetMinutes, includeWeekendsInStreak);
      setSettings(next);
      await loadProgress();
      setEditingGoal(false);
      setActionError(null);
    } catch (reason) {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleConfigureGitSync() {
    if (syncBusy) {
      return;
    }

    setSyncBusy(true);
    setSyncNotice(null);
    try {
      const next = await configureGitSyncRepository();
      if (next) {
        setSettings(next);
        setActionError(null);
        setSyncNotice("Git sync configured");
      }
    } catch (reason) {
      setActionError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSyncBusy(false);
    }
  }

  if (editingGoal) {
    return (
      <main className="desktop-main edit-screen">
        {(error || actionError) && <div className="error-banner">{error ?? actionError}</div>}
        <GitSyncControl
          configured={Boolean(settings.gitSyncRepoPath)}
          busy={syncBusy}
          notice={syncNotice}
          onConfigure={() => void handleConfigureGitSync()}
        />
        <GoalEditor
          settings={settings}
          onCancel={() => setEditingGoal(false)}
          onSave={(goal, reset, weekends) => void handleSaveGoal(goal, reset, weekends)}
        />
      </main>
    );
  }

  return (
    <main className="desktop-main">
      {(error || actionError) && <div className="error-banner">{error ?? actionError}</div>}
      <GitSyncControl
        configured={Boolean(settings.gitSyncRepoPath)}
        busy={syncBusy}
        notice={syncNotice}
        onConfigure={() => void handleConfigureGitSync()}
      />
      <section className="dashboard-grid">
        <FocusCard
          timer={timer}
          plannedMinutes={plannedMinutes}
          selectedMinutes={selectedMinutes}
          customMinutes={customMinutes}
          restMinutes={restMinutes}
          skipRest={skipRest}
          menuOpen={menuOpen}
          onMenuToggle={() => setMenuOpen((open) => !open)}
          onSelectMinutes={(minutes) => {
            setSelectedMinutes(minutes);
            setCustomMinutes(minutes);
            void persistFocusPreferences(minutes, restMinutes, skipRest).catch((reason) => {
              setActionError(reason instanceof Error ? reason.message : String(reason));
            });
          }}
          onCustomMinutes={handleFocusMinutesChange}
          onRestMinutesChange={handleRestMinutesChange}
          onSkipRestChange={handleSkipRestChange}
          onMainAction={() => {
            if (timer.status === "running") {
              void runAction(pauseFocusSession);
            } else if (timer.status === "paused") {
              void runAction(resumeFocusSession);
            } else {
              void runAction(() => startFocusSession(plannedMinutes, skipRest ? 0 : restMinutes));
            }
          }}
          onCancel={() => {
            setMenuOpen(false);
            void runAction(cancelFocusSession);
          }}
          onReset={() => {
            setMenuOpen(false);
            void runAction(resetFocusSession, true);
          }}
          onSwitchToFloating={() => void handleSwitchToFloating()}
        />
        <ProgressCard settings={settings} progress={progress} onEdit={() => setEditingGoal(true)} />
      </section>
    </main>
  );
}

interface FocusCardProps {
  timer: TimerSnapshot;
  plannedMinutes: number;
  selectedMinutes: number;
  customMinutes: number;
  restMinutes: number;
  skipRest: boolean;
  menuOpen: boolean;
  onMenuToggle(): void;
  onSelectMinutes(minutes: number): void;
  onCustomMinutes(minutes: number): void;
  onRestMinutesChange(minutes: number): void;
  onSkipRestChange(skip: boolean): void;
  onMainAction(): void;
  onCancel(): void;
  onReset(): void;
  onSwitchToFloating(): void;
}

function FocusCard({
  timer,
  plannedMinutes,
  selectedMinutes,
  customMinutes,
  restMinutes,
  skipRest,
  menuOpen,
  onMenuToggle,
  onSelectMinutes,
  onCustomMinutes,
  onRestMinutesChange,
  onSkipRestChange,
  onMainAction,
  onCancel,
  onReset,
  onSwitchToFloating,
}: FocusCardProps) {
  const isPreparing = timer.status === "idle" || timer.status === "completed" || timer.status === "cancelled";
  const actionKind: ActionIconKind = timer.status === "running" ? "pause" : "play";
  const actionLabel = timer.status === "running" ? "Pause" : timer.status === "paused" ? "Resume" : "Start";

  return (
    <article className="dashboard-card focus-card">
      <CardHeader title={isPreparing ? "" : "Focus session"}>
        <div className="header-actions">
          <button className="icon-button" onClick={onSwitchToFloating} aria-label="Switch to floating timer" title="Switch to floating timer">
            ⛶
          </button>
        </div>
      </CardHeader>

      {isPreparing ? (
        <div className="focus-center">
          <PrepareFocus
            selectedMinutes={selectedMinutes}
            customMinutes={customMinutes}
            restMinutes={restMinutes}
            skipRest={skipRest}
            onSelectMinutes={onSelectMinutes}
            onCustomMinutes={onCustomMinutes}
            onRestMinutesChange={onRestMinutesChange}
            onSkipRestChange={onSkipRestChange}
            onStart={onMainAction}
          />
        </div>
      ) : (
        <div className="focus-center active-focus-center">
          <div className="focus-dial-wrap">
            <TimerDial timer={timer} className="main-dial" />
          </div>

          <div className="focus-actions">
            <button className="round-primary" onClick={onMainAction} aria-label={actionLabel} title={actionLabel}>
              <ActionIcon kind={actionKind} />
            </button>
            {timer.status === "paused" && (
              <button className="round-secondary reset-button" onClick={onReset} aria-label="Save and reset" title="Save and reset">
                <ActionIcon kind="reset" />
              </button>
            )}
            <button
              className="round-secondary"
              data-focus-menu-toggle
              onClick={onMenuToggle}
              aria-label="More options"
              title="More options"
            >
              <DotsIcon />
            </button>
          </div>

          <p className="next-break">
            {timer.phase === "rest" ? (
              <strong>Resting</strong>
            ) : skipRest ? (
              <strong>Next: no break</strong>
            ) : (
              <>
                Next: <strong>{restMinutes} min break</strong>
              </>
            )}
          </p>
          <span className="planned-note">
            {timer.phase === "rest" ? `Break ${timer.restMinutes || restMinutes} min` : `Plan ${timer.plannedMinutes || plannedMinutes} min`}
          </span>
        </div>
      )}

      {!isPreparing && menuOpen && (
        <div className="focus-menu focus-tip-menu" data-focus-menu>
          <span>Click the timer or blue button to pause or resume.</span>
        </div>
      )}
    </article>
  );
}

interface PrepareFocusProps {
  selectedMinutes: number;
  customMinutes: number;
  restMinutes: number;
  skipRest: boolean;
  onSelectMinutes(minutes: number): void;
  onCustomMinutes(minutes: number): void;
  onRestMinutesChange(minutes: number): void;
  onSkipRestChange(skip: boolean): void;
  onStart(): void;
}

function PrepareFocus({
  selectedMinutes,
  customMinutes,
  restMinutes,
  skipRest,
  onSelectMinutes,
  onCustomMinutes,
  onRestMinutesChange,
  onSkipRestChange,
  onStart,
}: PrepareFocusProps) {
  const displayMinutes = selectedMinutes || customMinutes;

  function increaseFocusMinutes() {
    onCustomMinutes(nextQuarterHour(displayMinutes));
  }

  function decreaseFocusMinutes() {
    onCustomMinutes(previousQuarterHour(displayMinutes));
  }

  return (
    <section className="prepare-focus">
      <h2>Ready to focus</h2>
      <p>FocusTrail helps you set a focused block and an optional short break so you can come back with a clearer head.</p>

      <div className="minute-stepper">
        <div>
          <input
            aria-label="Focus duration in minutes"
            type="number"
            inputMode="numeric"
            min={1}
            max={720}
            value={displayMinutes}
            onChange={(event) => onCustomMinutes(Number(event.currentTarget.value))}
          />
          <span>min</span>
        </div>
        <div className="stepper-buttons">
          <button onClick={increaseFocusMinutes} aria-label="Increase to the next 15-minute step">
            <span className="stepper-chevron up" />
          </button>
          <button onClick={decreaseFocusMinutes} aria-label="Decrease to the previous 15-minute step">
            <span className="stepper-chevron down" />
          </button>
        </div>
      </div>

      <label className="rest-config">
        <span>Break duration</span>
        <input
          type="number"
          inputMode="numeric"
          min={1}
          max={120}
          value={skipRest ? "" : restMinutes}
          disabled={skipRest}
          onChange={(event) => onRestMinutesChange(clampMinutes(Number(event.currentTarget.value)))}
        />
        <span>min</span>
      </label>

      <label className="skip-break">
        <input type="checkbox" checked={skipRest} onChange={(event) => onSkipRestChange(event.currentTarget.checked)} />
        <span>Skip break</span>
      </label>

      <button className="start-focus-button" onClick={onStart}>
        <ActionIcon kind="play" />
        <span>Start focus session</span>
      </button>

      <div className="prepare-presets" aria-label="Quick duration choices">
        {[25, 30, 45, 60].map((minutes) => (
          <button key={minutes} className={selectedMinutes === minutes ? "selected" : ""} onClick={() => onSelectMinutes(minutes)}>
            {minutes}
          </button>
        ))}
      </div>
    </section>
  );
}

interface ProgressCardProps {
  settings: Settings;
  progress: DailyProgress | null;
  onEdit(): void;
}

function ProgressCard({ settings, progress, onEdit }: ProgressCardProps) {
  const goalMinutes = progress?.dailyGoalMinutes ?? settings.dailyGoalMinutes;
  const ratio = progress ? completedRatio(progress) : 0;
  const goal = goalDisplay(goalMinutes);

  return (
    <article className="dashboard-card progress-card">
      <CardHeader title="Daily progress">
        <button className="icon-button" onClick={onEdit} aria-label="Edit daily goal" title="Edit daily goal">
          ✎
        </button>
      </CardHeader>

      <div className="progress-layout">
        <MiniStat label="Yesterday" seconds={progress?.yesterdayCompletedSeconds ?? 0} />
        <div className="progress-center">
          <ProgressRing ratio={ratio}>
            <span>Daily goal</span>
            <strong>{goal.value}</strong>
            <span>{goal.unit}</span>
          </ProgressRing>
          <p className="completed-text">Completed: {formatHoursMinutes(progress?.todayCompletedSeconds ?? 0)}</p>
          <p className="rested-text">Rested: {formatHoursMinutes(progress?.todayRestSeconds ?? 0)}</p>
        </div>
        <MiniStat label="Streak" value={`${progress?.streakDays ?? 0} days`} />
      </div>
    </article>
  );
}

interface GitSyncControlProps {
  configured: boolean;
  busy: boolean;
  notice: string | null;
  onConfigure(): void;
}

function GitSyncControl({ configured, busy, notice, onConfigure }: GitSyncControlProps) {
  return (
    <div className="git-sync-control">
      <button
        className={configured ? "git-sync-button configured" : "git-sync-button"}
        onClick={onConfigure}
        disabled={busy}
        aria-label="Configure Git sync"
        title={configured ? "Git sync configured" : "Configure Git sync"}
      >
        <span aria-hidden="true">⚙</span>
        <span className="git-sync-dot" aria-hidden="true" />
      </button>
      {notice && <span className="git-sync-notice">{notice}</span>}
    </div>
  );
}

interface CardHeaderProps {
  title: string;
  children: ReactNode;
}

function CardHeader({ title, children }: CardHeaderProps) {
  return (
    <header className="card-header">
      {title ? <h2>{title}</h2> : <span />}
      {children}
    </header>
  );
}

interface MiniStatProps {
  label: string;
  value?: string;
  seconds?: number;
}

function MiniStat({ label, value, seconds }: MiniStatProps) {
  const time = typeof seconds === "number" ? splitHoursMinutes(seconds) : null;
  const [main, unit = ""] = value?.split(" ") ?? [];

  return (
    <div className="mini-stat">
      <span>{label}</span>
      {time ? (
        <>
          <strong>{time.hours}</strong>
          <span>h {time.minutes} min</span>
        </>
      ) : (
        <>
          <strong>{main}</strong>
          {unit && <span>{unit}</span>}
        </>
      )}
    </div>
  );
}

interface ProgressRingProps {
  ratio: number;
  children: ReactNode;
}

function ProgressRing({ ratio, children }: ProgressRingProps) {
  const radius = 82;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference * (1 - Math.min(1, Math.max(0, ratio)));

  return (
    <div className="daily-ring">
      <svg viewBox="0 0 190 190" aria-hidden="true">
        <circle className="daily-ring-bg" cx="95" cy="95" r={radius} />
        <circle className="daily-ring-progress" cx="95" cy="95" r={radius} strokeDasharray={circumference} strokeDashoffset={offset} />
      </svg>
      <div>{children}</div>
    </div>
  );
}

interface GoalEditorProps {
  settings: Settings;
  onCancel(): void;
  onSave(dailyGoalMinutes: number, dailyResetMinutes: number, includeWeekendsInStreak: boolean): void;
}

function GoalEditor({ settings, onCancel, onSave }: GoalEditorProps) {
  const [dailyGoalMinutes, setDailyGoalMinutes] = useState(settings.dailyGoalMinutes);
  const [resetHour, setResetHour] = useState(resetTimeParts(settings.dailyResetMinutes).hour);
  const [resetMinute, setResetMinute] = useState(resetTimeParts(settings.dailyResetMinutes).minute);
  const [includeWeekendsInStreak, setIncludeWeekendsInStreak] = useState(settings.includeWeekendsInStreak);
  const options = uniqueNumbers([60, 120, 180, 240, 300, 360, 420, 480, 540, 600, 660, 720, settings.dailyGoalMinutes]);

  useEffect(() => {
    setDailyGoalMinutes(settings.dailyGoalMinutes);
    setResetHour(resetTimeParts(settings.dailyResetMinutes).hour);
    setResetMinute(resetTimeParts(settings.dailyResetMinutes).minute);
    setIncludeWeekendsInStreak(settings.includeWeekendsInStreak);
  }, [settings.dailyGoalMinutes, settings.dailyResetMinutes, settings.includeWeekendsInStreak]);

  function saveGoal() {
    onSave(clampMinutes(dailyGoalMinutes), resetTimeMinutes(resetHour, resetMinute), includeWeekendsInStreak);
  }

  return (
    <section className="goal-editor" aria-label="Edit daily goal">
      <h1>Edit daily goal</h1>

      <label className="form-field">
        <span>Daily goal</span>
        <select value={dailyGoalMinutes} onChange={(event) => setDailyGoalMinutes(Number(event.currentTarget.value))}>
          {options.map((minutes) => (
            <option key={minutes} value={minutes}>
              {goalDisplay(minutes).value} {goalDisplay(minutes).unit}
            </option>
          ))}
        </select>
      </label>

      <label className="form-field">
        <span>Daily reset time</span>
        <span className="split-time" aria-label="Daily reset time">
          <input
            aria-label="Daily reset hour"
            type="number"
            inputMode="numeric"
            min={0}
            max={23}
            value={resetHour}
            onChange={(event) => setResetHour(event.currentTarget.value)}
            onBlur={() => setResetHour(normalizeTimePart(resetHour, 23, false))}
          />
          <input
            aria-label="Daily reset minute"
            type="number"
            inputMode="numeric"
            min={0}
            max={59}
            value={resetMinute}
            onChange={(event) => setResetMinute(event.currentTarget.value)}
            onBlur={() => setResetMinute(normalizeTimePart(resetMinute, 59, true))}
          />
        </span>
      </label>

      <label className="check-line">
        <input
          type="checkbox"
          checked={includeWeekendsInStreak}
          onChange={(event) => setIncludeWeekendsInStreak(event.currentTarget.checked)}
        />
        <span>Include weekends in streaks</span>
      </label>

      <footer className="editor-footer">
        <button className="save-button" onClick={saveGoal}>
          Save
        </button>
        <button className="cancel-button" onClick={onCancel}>
          Cancel
        </button>
      </footer>
    </section>
  );
}

function goalDisplay(minutes: number): { value: string; unit: string } {
  return { value: String(Math.round(minutes / 60)), unit: "h" };
}

function resetTimeParts(minutes: number): { hour: string; minute: string } {
  const safeMinutes = Math.min(23 * 60 + 59, Math.max(0, Math.round(minutes)));
  return {
    hour: String(Math.floor(safeMinutes / 60)),
    minute: String(safeMinutes % 60).padStart(2, "0"),
  };
}

function resetTimeMinutes(hour: string, minute: string): number {
  return timePartNumber(hour, 23) * 60 + timePartNumber(minute, 59);
}

function normalizeTimePart(value: string, max: number, pad: boolean): string {
  const normalized = String(timePartNumber(value, max));
  return pad ? normalized.padStart(2, "0") : normalized;
}

function timePartNumber(value: string, max: number): number {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) {
    return 0;
  }

  return Math.min(max, Math.max(0, parsed));
}

function uniqueNumbers(values: number[]): number[] {
  return [...new Set(values.map((value) => Math.max(60, Math.min(720, Math.round(value / 60) * 60))))].sort((a, b) => a - b);
}

function nextQuarterHour(minutes: number): number {
  return clampMinutes(Math.ceil((minutes + 1) / 15) * 15);
}

function previousQuarterHour(minutes: number): number {
  return clampMinutes(Math.max(15, Math.floor((minutes - 1) / 15) * 15));
}

type ActionIconKind = "pause" | "play" | "reset";

interface ActionIconProps {
  kind: ActionIconKind;
}

function ActionIcon({ kind }: ActionIconProps) {
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
