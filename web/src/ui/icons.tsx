/* Inline glyphs used by the Beautiful UI primitives (see ./LICENSE). */
import type { ReactNode } from "react";

type IconProps = { size?: number; className?: string; strokeWidth?: number };

function Stroke({ size = 14, className, strokeWidth = 2, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      {children}
    </svg>
  );
}

export function SparkleIcon({ size = 14, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden>
      <path d="M12 2l2.4 7.2L22 12l-7.6 2.8L12 22l-2.4-7.2L2 12l7.6-2.8z" />
    </svg>
  );
}

export const ChevronIcon = (p: IconProps) => (
  <Stroke strokeWidth={2.2} {...p}>
    <path d="M6 9l6 6 6-6" />
  </Stroke>
);

export const CheckIcon = (p: IconProps) => (
  <Stroke strokeWidth={3.5} {...p}>
    <path d="M20 6L9 17l-5-5" />
  </Stroke>
);

export const XIcon = (p: IconProps) => (
  <Stroke strokeWidth={3.5} {...p}>
    <path d="M18 6L6 18M6 6l12 12" />
  </Stroke>
);

export const PauseIcon = (p: IconProps) => (
  <Stroke strokeWidth={3.5} {...p}>
    <path d="M9 6v12M15 6v12" />
  </Stroke>
);

export const RunIcon = (p: IconProps) => (
  <Stroke {...p}>
    <path d="M4 17l6-5-6-5M12 19h8" />
  </Stroke>
);

export const WriteIcon = (p: IconProps) => (
  <Stroke {...p}>
    <path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z" />
  </Stroke>
);

export const ReadIcon = (p: IconProps) => (
  <Stroke {...p}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <path d="M14 2v6h6" />
  </Stroke>
);

export const SearchIcon = (p: IconProps) => (
  <Stroke {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M21 21l-4.3-4.3" />
  </Stroke>
);

export const GlobeIcon = (p: IconProps) => (
  <Stroke {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3.5 12h17M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
  </Stroke>
);

export const ToolIcon = (p: IconProps) => (
  <Stroke {...p}>
    <path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.4-.6-.6-2.4z" />
  </Stroke>
);

export const CopyIcon = (p: IconProps) => (
  <Stroke {...p}>
    <rect x="9" y="9" width="12" height="12" rx="2.5" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </Stroke>
);

export const SidebarIcon = (p: IconProps) => (
  <Stroke {...p}>
    <rect x="3" y="4" width="18" height="16" rx="3" />
    <path d="M9 4v16" />
  </Stroke>
);

export const EyeIcon = (p: IconProps) => (
  <Stroke {...p}>
    <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" />
    <circle cx="12" cy="12" r="3" />
  </Stroke>
);
