import { minuteLabel } from "../domain/session";
import type { TimerSnapshot } from "../domain/session";

interface TimerDialProps {
  timer: TimerSnapshot;
  className?: string;
  onClick?(): void;
}

const tickCount = 26;
const normalTick = { outerY: 40, innerY: 55 };
const compactTick = { outerY: 44, innerY: 52 };

export function TimerDial({ timer, className = "", onClick }: TimerDialProps) {
  const activeTicks = Math.round(Math.min(1, Math.max(0, timer.progress)) * tickCount);
  const remaining = minuteLabel(timer.remainingSeconds);
  const isButton = typeof onClick === "function";
  const content = (
    <>
      <svg viewBox="0 0 240 240" aria-hidden="true">
        <circle className="dial-track" cx="120" cy="120" r="91" />
        {Array.from({ length: tickCount }, (_, index) => {
          const active = index < activeTicks;
          const rotation = `rotate(${90 + index * (360 / tickCount)} 120 120)`;
          return (
            <g key={index}>
              <line
                className={active ? "dial-tick dial-tick-long active" : "dial-tick dial-tick-long"}
                x1="120"
                y1={normalTick.outerY}
                x2="120"
                y2={normalTick.innerY}
                transform={rotation}
              />
              <line
                className={active ? "dial-tick dial-tick-short active" : "dial-tick dial-tick-short"}
                x1="120"
                y1={compactTick.outerY}
                x2="120"
                y2={compactTick.innerY}
                transform={rotation}
              />
            </g>
          );
        })}
      </svg>
      <span className="dial-time">
        <strong>{remaining.value}</strong>
        <span>{remaining.unit}</span>
      </span>
    </>
  );

  if (isButton) {
    return (
      <button className={`timer-dial ${className}`} onClick={onClick} aria-label="Pause or resume">
        {content}
      </button>
    );
  }

  return <div className={`timer-dial ${className}`}>{content}</div>;
}
