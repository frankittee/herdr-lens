/* Line-numbered listing from Beautiful UI's Code Block (see ./LICENSE). */
import { useState } from "react";
import { CheckIcon, CopyIcon } from "./icons";

export function CodeBlock({ code, label }: { code: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const lines = code.replace(/\n$/, "").split("\n");

  const copy = () => {
    navigator.clipboard?.writeText(code).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div className="w-full overflow-hidden rounded-card bg-surface shadow-card">
      <div className="flex h-9 items-center gap-2 border-b border-line px-3.5 text-[12px]">
        <span className="truncate font-mono leading-none text-ink-2">{label || "code"}</span>
        <button
          type="button"
          aria-label="Copy code"
          onClick={copy}
          className={`-mr-1 ml-auto flex h-6 items-center gap-1 rounded-[6px] px-1.5 text-[12px] font-medium transition-colors duration-100 hover:bg-hover ${
            copied ? "text-green" : "text-ink-3 hover:text-ink"
          }`}
        >
          {copied ? <CheckIcon size={11} strokeWidth={3} /> : <CopyIcon size={11} />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <div className="relative overflow-x-auto py-2.5 font-mono text-[12px] leading-[1.65] text-ink-2">
        <span className="pointer-events-none absolute inset-y-0 left-7 w-px bg-line" />
        {lines.map((line, i) => (
          <div key={i} className="grid grid-cols-[28px_minmax(0,1fr)] items-start">
            <span className="select-none text-center text-[11px] text-ink-3 tabular-nums">{i + 1}</span>
            <span className="pr-3 pl-2 break-words whitespace-pre-wrap">{line || " "}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
