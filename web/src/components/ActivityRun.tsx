/* A run of reasoning and tool calls, rendered like Beautiful UI's Tool Chips (see ../ui/LICENSE). */
import { memo, useState, type ReactNode } from "react";
import type { Message } from "../api";
import { Expand } from "../ui/Expand";
import {
  ChevronIcon,
  GlobeIcon,
  ReadIcon,
  RunIcon,
  SearchIcon,
  SparkleIcon,
  ToolIcon,
  WriteIcon,
} from "../ui/icons";
import { ShimmerText } from "../ui/loading";

const MAX_DETAIL_LINES = 200;

type Kind = "think" | "read" | "search" | "write" | "run" | "web" | "other";

const KIND_BY_TOOL: Record<string, Kind> = {
  read: "read",
  ls: "read",
  glob: "read",
  view: "read",
  grep: "search",
  search: "search",
  websearch: "web",
  fetchurl: "web",
  webfetch: "web",
  edit: "write",
  multiedit: "write",
  create: "write",
  write: "write",
  applypatch: "write",
  apply_patch: "write",
  execute: "run",
  bash: "run",
  shell: "run",
  exec: "run",
};

const KIND_ICONS: Record<Kind, ReactNode> = {
  think: <SparkleIcon size={12} />,
  read: <ReadIcon size={13} />,
  search: <SearchIcon size={13} />,
  write: <WriteIcon size={13} />,
  run: <RunIcon size={13} />,
  web: <GlobeIcon size={13} />,
  other: <ToolIcon size={13} />,
};

function kindOf(message: Message): Kind {
  if (message.role === "reasoning") return "think";
  const name = (message.tool?.name ?? message.title ?? "").toLowerCase();
  return KIND_BY_TOOL[name] ?? "other";
}

function labelOf(message: Message): string {
  if (message.role === "reasoning") return "Thought";
  return message.tool?.name ?? message.title?.split(" ")[0] ?? "Tool";
}

/* The one-line summary shown in the chip: the call without the tool name prefix. */
function chipOf(message: Message): string {
  if (message.role === "reasoning") return firstLine(message.text);
  const title = message.title ?? "";
  const name = message.tool?.name;
  const summary = name && title.startsWith(name) ? title.slice(name.length).trim() : title;
  return firstLine(summary);
}

function firstLine(text: string): string {
  return text.trim().split("\n", 1)[0] ?? "";
}

type DiffLine = { text: string; tone: "add" | "del" };

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

/* Edit-style tools carry the change in their input, which reads better than their output. */
function diffOf(message: Message): DiffLine[] | null {
  const input = message.tool?.input;
  if (kindOf(message) !== "write" || !input || typeof input !== "object") return null;
  const fields = input as Record<string, unknown>;
  const removed = asString(fields.old_str) ?? asString(fields.old_string) ?? "";
  const added = asString(fields.new_str) ?? asString(fields.new_string) ?? asString(fields.content);
  if (added === null) return null;
  const toLines = (text: string, tone: DiffLine["tone"]) =>
    text ? text.split("\n").map((line) => ({ text: line, tone })) : [];
  return [...toLines(removed, "del"), ...toLines(added, "add")];
}

function clip<T>(items: T[]): { shown: T[]; hidden: number } {
  return { shown: items.slice(0, MAX_DETAIL_LINES), hidden: Math.max(0, items.length - MAX_DETAIL_LINES) };
}

function MoreLines({ count }: { count: number }) {
  if (count === 0) return null;
  return <div className="px-2.5 pt-1 text-[11px] text-ink-3">… {count} more lines</div>;
}

function DiffDetail({ lines }: { lines: DiffLine[] }) {
  const { shown, hidden } = clip(lines);
  return (
    <div className="overflow-hidden rounded-[8px] bg-surface py-1 font-mono text-[11.5px] leading-[1.7] shadow-hairline">
      {shown.map((line, i) => (
        <div
          key={i}
          className={`flex gap-2 px-2.5 ${line.tone === "add" ? "bg-green-tint text-green" : "bg-red-tint text-red"}`}
        >
          <span className="w-3 shrink-0 select-none">{line.tone === "add" ? "+" : "−"}</span>
          <span className="min-w-0 break-words whitespace-pre-wrap">{line.text || " "}</span>
        </div>
      ))}
      <MoreLines count={hidden} />
    </div>
  );
}

function OutputDetail({ text, error }: { text: string; error: boolean }) {
  const { shown, hidden } = clip(text.replace(/\n$/, "").split("\n"));
  return (
    <div
      className={`max-h-96 overflow-auto rounded-[8px] px-2.5 py-1.5 font-mono text-[11.5px] leading-[1.6] shadow-hairline ${
        error ? "bg-red-tint text-red" : "bg-surface text-ink-2"
      }`}
    >
      <pre className="break-words whitespace-pre-wrap">{shown.join("\n")}</pre>
      <MoreLines count={hidden} />
    </div>
  );
}

function RowDetail({ message }: { message: Message }) {
  if (message.role === "reasoning") {
    return <p className="text-[12.5px] leading-relaxed whitespace-pre-wrap text-ink-2">{message.text}</p>;
  }
  const tool = message.tool;
  const diff = diffOf(message);
  const summary = chipOf(message);
  const fullTitle = message.title ?? "";
  return (
    <div className="flex flex-col gap-1.5">
      {fullTitle.includes("\n") || summary.length > 80 ? (
        <pre className="font-mono text-[11.5px] break-words whitespace-pre-wrap text-ink">{fullTitle}</pre>
      ) : null}
      {diff && diff.length > 0 && <DiffDetail lines={diff} />}
      {message.text && (!diff || tool?.is_error) ? <OutputDetail text={message.text} error={!!tool?.is_error} /> : null}
      {!message.text && !diff && (
        <span className="text-[11.5px] text-ink-3">{tool?.pending ? "Waiting for output…" : "No output"}</span>
      )}
    </div>
  );
}

function ActivityRow({ message, open, onToggle }: { message: Message; open: boolean; onToggle: () => void }) {
  const kind = kindOf(message);
  const pending = !!message.tool?.pending;
  const error = !!message.tool?.is_error;
  const chip = chipOf(message);
  const mono = kind !== "think" && kind !== "other";

  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="group/row -mx-[3px] flex h-7 w-[calc(100%+6px)] min-w-0 items-center gap-2 rounded-control px-[3px] text-left transition-colors duration-100 hover:bg-hover-2"
      >
        <span className={`relative flex size-4 shrink-0 items-center justify-center ${error ? "text-red" : "text-ink-3"}`}>
          {pending ? (
            <span
              className="size-3 rounded-full border-[1.5px] border-line-strong border-t-ink-2"
              style={{ animation: "spin 700ms linear infinite" }}
            />
          ) : (
            <span className={`flex transition-opacity duration-100 group-hover/row:opacity-0 ${open ? "opacity-0" : ""}`}>
              {KIND_ICONS[kind]}
            </span>
          )}
          {!pending && (
            <span
              className={`absolute flex transition-[opacity,transform] duration-150 group-hover/row:opacity-100 ${
                open ? "opacity-100" : "opacity-0"
              }`}
              style={{ transform: open ? "rotate(0deg)" : "rotate(-90deg)" }}
            >
              <ChevronIcon size={12} />
            </span>
          )}
        </span>
        <span className={`shrink-0 text-[12.5px] font-medium ${error ? "text-red" : "text-ink"}`}>{labelOf(message)}</span>
        {chip && !(open && kind === "think") && (
          <span
            className={`inline-flex h-5.5 min-w-0 items-center truncate rounded-chip px-1.5 text-[11.5px] shadow-hairline ${
              error ? "bg-red-tint text-red" : "bg-field text-ink-2"
            } ${mono ? "font-mono" : ""}`}
          >
            <span className="truncate">{chip}</span>
          </span>
        )}
        {error && <span className="ml-auto shrink-0 text-[11px] font-medium text-red">Failed</span>}
      </button>
      <Expand open={open}>
        <div className="mt-0.5 mb-1.5 ml-2 border-l border-line py-0.5 pl-3.5">
          <RowDetail message={message} />
        </div>
      </Expand>
    </div>
  );
}

function headerText(messages: Message[]): string {
  const tools = messages.filter((message) => message.role === "tool").length;
  const thoughts = messages.length - tools;
  const parts = [];
  if (tools) parts.push(`${tools} tool call${tools === 1 ? "" : "s"}`);
  if (thoughts) parts.push(`${thoughts} thought${thoughts === 1 ? "" : "s"}`);
  return parts.join(", ");
}

export const ActivityRun = memo(function ActivityRun({ messages, live }: { messages: Message[]; live: boolean }) {
  const [open, setOpen] = useState(false);
  const [openRows, setOpenRows] = useState<Set<number>>(() => new Set());

  const toggleRow = (index: number) =>
    setOpenRows((current) => {
      const next = new Set(current);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });

  return (
    <div className="w-full" style={{ animation: "fade-up 300ms var(--ease-out-strong) both" }}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
        className="-mx-1.5 flex w-fit items-center gap-1.5 rounded-control px-1.5 py-1 text-[12.5px] text-ink-2 transition-colors duration-100 hover:bg-hover-2"
      >
        <span className="flex transition-transform duration-200" style={{ transform: open ? "rotate(0deg)" : "rotate(-90deg)" }}>
          <ChevronIcon size={12} />
        </span>
        {live ? <ShimmerText>{headerText(messages)}</ShimmerText> : <span className="tabular-nums">{headerText(messages)}</span>}
      </button>
      <Expand open={open}>
        <div className="-mx-1 px-1.5 pb-1">
          <div className="mt-1 flex flex-col gap-0.5">
            {messages.map((message, index) => (
              <ActivityRow key={index} message={message} open={openRows.has(index)} onToggle={() => toggleRow(index)} />
            ))}
          </div>
        </div>
      </Expand>
    </div>
  );
});
