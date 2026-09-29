/* Pixel-grid loader and shimmer label from Beautiful UI's Loading State (see ./LICENSE). */
import type { ReactNode } from "react";

const CHEVRON_DELAYS = Array.from({ length: 9 }, (_, i) => {
  const row = Math.floor(i / 3);
  const col = i % 3;
  return (col + Math.abs(row - 1)) * 90;
});

export function LoaderGrid() {
  return (
    <span aria-hidden className="grid shrink-0 grid-cols-[repeat(3,4px)] gap-[1.5px]">
      {CHEVRON_DELAYS.map((delay, index) => (
        <span
          key={index}
          className="size-[4px] rounded-[1px] bg-ink"
          style={{ opacity: 0.15, animation: `pixel-on 650ms ease-in-out ${delay}ms infinite` }}
        />
      ))}
    </span>
  );
}

export function ShimmerText({ children }: { children: ReactNode }) {
  return (
    <span
      className="bg-clip-text text-[13px] font-medium whitespace-nowrap text-transparent"
      style={{
        backgroundImage: "linear-gradient(90deg, var(--ink-3) 35%, var(--ink) 50%, var(--ink-3) 65%)",
        backgroundSize: "200% 100%",
        animation: "shimmer-text 1.4s linear infinite",
      }}
    >
      {children}
    </span>
  );
}

export function LoadingLine({ label, meta }: { label: string; meta?: string }) {
  return (
    <div role="status" className="flex w-fit items-center gap-2.5">
      <LoaderGrid />
      <ShimmerText>{label}</ShimmerText>
      {meta && <span className="font-mono text-[12px] text-ink-3 tabular-nums">{meta}</span>}
    </div>
  );
}
