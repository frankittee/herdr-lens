/* Status marks and pills from Beautiful UI's Task Rows (see ./LICENSE). */
import type { ReactNode } from "react";
import { CheckIcon, PauseIcon } from "./icons";

export function SpinnerRing({ active, size = 22 }: { active?: boolean; size?: number }) {
  const stroke = 2;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  return (
    <span className="relative inline-flex shrink-0" style={{ width: size, height: size }}>
      <svg
        width={size}
        height={size}
        className="absolute inset-0"
        style={active ? { animation: "spin 1.1s linear infinite" } : undefined}
        aria-hidden
      >
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--line-strong)" strokeWidth={stroke} />
        {active && (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke="var(--accent)"
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={`${c * 0.28} ${c * 0.72}`}
          />
        )}
      </svg>
    </span>
  );
}

type Tone = "green" | "red" | "orange";

const BADGE_TONES: Record<Tone, string> = {
  green: "bg-green",
  red: "bg-red",
  orange: "bg-orange",
};

export function ToneBadge({ tone, children }: { tone: Tone; children: ReactNode }) {
  return (
    <span
      className={`flex size-5.5 shrink-0 items-center justify-center rounded-full text-white ${BADGE_TONES[tone]}`}
      style={{ animation: "pop-in 300ms var(--ease-out-strong) both" }}
    >
      {children}
    </span>
  );
}

const PILL_TONES: Record<Tone | "muted" | "accent", string> = {
  green: "bg-green-tint text-green",
  red: "bg-red-tint text-red",
  orange: "bg-orange-tint text-orange",
  accent: "bg-accent-tint text-accent-ink",
  muted: "bg-field text-ink-2",
};

export function Pill({ tone, children }: { tone: keyof typeof PILL_TONES; children: ReactNode }) {
  return (
    <span
      className={`inline-flex h-5.5 shrink-0 items-center rounded-full px-2 text-[11.5px] font-medium ${PILL_TONES[tone]}`}
    >
      {children}
    </span>
  );
}

/* Herdr agent states: idle, working, blocked, done, unknown. */
export const STATUS_LABELS: Record<string, string> = {
  working: "Working",
  blocked: "Needs input",
  done: "Done",
  idle: "Idle",
  unknown: "Unknown",
};

export function StatusMark({ status }: { status: string }) {
  switch (status) {
    case "working":
      return <SpinnerRing active />;
    case "blocked":
      return (
        <ToneBadge tone="orange">
          <PauseIcon size={11} />
        </ToneBadge>
      );
    case "done":
      return (
        <ToneBadge tone="green">
          <CheckIcon size={12} />
        </ToneBadge>
      );
    default:
      return <SpinnerRing />;
  }
}

export function StatusPill({ status }: { status: string }) {
  const label = STATUS_LABELS[status] ?? status;
  const tone = status === "working" ? "accent" : status === "blocked" ? "orange" : status === "done" ? "green" : "muted";
  return <Pill tone={tone}>{label}</Pill>;
}
