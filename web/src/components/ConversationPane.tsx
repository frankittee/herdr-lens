import { useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Conversation, Message } from "../api";
import { ChevronIcon } from "../ui/icons";
import { LoadingLine } from "../ui/loading";
import { Pill, StatusPill } from "../ui/status";
import { ActivityRun } from "./ActivityRun";
import { Markdown } from "./Markdown";

/* Pixels from the bottom that still count as "following" the live transcript. */
const FOLLOW_THRESHOLD = 80;

type Block =
  | { kind: "message"; message: Message }
  | { kind: "activity"; messages: Message[] };

/* Consecutive reasoning and tool messages collapse into one activity run. */
function toBlocks(messages: Message[]): Block[] {
  const blocks: Block[] = [];
  for (const message of messages) {
    const isActivity = message.role === "reasoning" || message.role === "tool";
    const last = blocks[blocks.length - 1];
    if (isActivity && last?.kind === "activity") last.messages.push(message);
    else if (isActivity) blocks.push({ kind: "activity", messages: [message] });
    else blocks.push({ kind: "message", message });
  }
  return blocks;
}

function formatTime(timestamp?: string): string | null {
  if (!timestamp) return null;
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function UserBubble({ message }: { message: Message }) {
  const time = formatTime(message.timestamp);
  return (
    <div className="flex flex-col items-end gap-1 pl-8 @xl:pl-16" style={{ animation: "fade-up 300ms var(--ease-out-strong) both" }}>
      <div className="max-w-full rounded-xl bg-field px-3.5 py-2 text-[13.5px] leading-[1.5] break-words whitespace-pre-wrap text-ink">
        {message.text}
      </div>
      {time && <span className="pr-1 text-[11px] text-ink-3 tabular-nums">{time}</span>}
    </div>
  );
}

function MessageBlock({ message }: { message: Message }) {
  switch (message.role) {
    case "user":
      return <UserBubble message={message} />;
    case "assistant":
      return <Markdown text={message.text} />;
    case "system":
      return (
        <div className="flex items-center gap-3 text-[11.5px] text-ink-3">
          <span className="h-px flex-1 bg-line" />
          <span className="max-w-[70%] text-center">{message.text}</span>
          <span className="h-px flex-1 bg-line" />
        </div>
      );
    default:
      return (
        <pre className="overflow-x-auto rounded-card bg-surface p-3 font-mono text-[12px] leading-[1.6] whitespace-pre-wrap text-ink-2 shadow-card">
          {message.text}
        </pre>
      );
  }
}

function Header({ conversation }: { conversation: Conversation }) {
  return (
    <header className="flex shrink-0 flex-col gap-1.5 border-b border-line bg-page/80 px-4 py-3 backdrop-blur @2xl:px-6">
      <div className="flex min-w-0 items-center gap-2">
        <h1 className="truncate text-[15px] font-semibold tracking-tight text-ink">
          {conversation.title || "Untitled session"}
        </h1>
        <StatusPill status={conversation.status} />
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-ink-2">
        <Pill tone="muted">{conversation.agent}</Pill>
        <span className="font-mono text-ink-3">{conversation.pane_id}</span>
        {conversation.cwd && (
          <span className="hidden min-w-0 truncate font-mono text-ink-3 @lg:inline">{conversation.cwd}</span>
        )}
        <span className="text-ink-3 tabular-nums">{conversation.messages.length} messages</span>
        {conversation.source === "terminal" && (
          <Pill tone="orange">Terminal scrollback, may be incomplete</Pill>
        )}
      </div>
    </header>
  );
}

export function ConversationPane({ conversation, error }: { conversation: Conversation | null; error: string | null }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [showJump, setShowJump] = useState(false);
  const blocks = useMemo(() => toBlocks(conversation?.messages ?? []), [conversation]);
  const working = conversation?.status === "working";

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && following.current) el.scrollTop = el.scrollHeight;
  }, [blocks]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_THRESHOLD;
    following.current = atBottom;
    setShowJump(!atBottom);
  };

  const jumpToLatest = () => {
    const el = scrollRef.current;
    el?.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  };

  if (!conversation) {
    return (
      <main className="flex min-w-0 flex-1 items-center justify-center bg-page">
        {error ? (
          <p className={`max-w-md px-6 text-center text-[13px] ${error === "This harness is not supported yet" ? "text-ink-2" : "text-red"}`}>
            {error === "This harness is not supported yet" ? error : `Could not load the conversation: ${error}`}
          </p>
        ) : (
          <LoadingLine label="Loading conversation" />
        )}
      </main>
    );
  }

  return (
    <main className="@container relative flex min-w-0 flex-1 flex-col bg-page">
      <Header conversation={conversation} />
      {error && (
        <div className="shrink-0 border-b border-line bg-red-tint px-4 py-1.5 text-[12px] text-red @2xl:px-6">
          Could not refresh: {error}. Showing the last loaded transcript.
        </div>
      )}
      <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-5 px-4 py-4 @2xl:px-6 @2xl:py-6">
          {blocks.map((block, index) =>
            block.kind === "activity" ? (
              <ActivityRun key={index} messages={block.messages} live={working && index === blocks.length - 1} />
            ) : (
              <MessageBlock key={index} message={block.message} />
            ),
          )}
          {blocks.length === 0 && <p className="text-center text-[13px] text-ink-3">No messages yet.</p>}
          {working && <LoadingLine label={`${conversation.agent} is working`} />}
        </div>
      </div>
      {showJump && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="absolute bottom-5 left-1/2 flex h-8 -translate-x-1/2 items-center gap-1.5 rounded-full bg-surface px-3 text-[12px] font-medium text-ink-2 shadow-raised transition-colors hover:text-ink"
          style={{ animation: "pop-in 200ms var(--ease-out-strong) both" }}
        >
          <ChevronIcon size={12} />
          Jump to latest
        </button>
      )}
    </main>
  );
}
