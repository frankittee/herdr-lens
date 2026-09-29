/* Height-animated disclosure body from Beautiful UI's Thinking and Tool Chips (see ./LICENSE). */
import type { ReactNode } from "react";

export function Expand({ open, children }: { open: boolean; children: ReactNode }) {
  return (
    <div
      className="grid transition-[grid-template-rows,opacity] duration-300"
      style={{
        gridTemplateRows: open ? "1fr" : "0fr",
        opacity: open ? 1 : 0,
        transitionTimingFunction: "var(--ease-out-strong)",
      }}
    >
      {/* Collapsed content is not rendered, which keeps long transcripts cheap. */}
      <div className="min-h-0 overflow-hidden">{open ? children : null}</div>
    </div>
  );
}
