import { useEffect, useState } from "react";

/* Mirrors `Conversation` in src/conversation.rs. */
export type Role = "user" | "assistant" | "reasoning" | "tool" | "system" | "terminal";

export type AgentStatus = "idle" | "working" | "blocked" | "done" | "unknown";

export type ToolCall = {
  name: string;
  input: unknown;
  pending: boolean;
  is_error: boolean;
};

export type Message = {
  role: Role;
  title?: string;
  text: string;
  timestamp?: string;
  tool?: ToolCall;
};

export type AgentSession = {
  agent: string;
  kind: string;
  source: string;
  value: string;
};

export type Conversation = {
  agent: string;
  pane_id: string;
  workspace_id: string;
  tab_id: string;
  cwd: string | null;
  status: AgentStatus | string;
  session: AgentSession | null;
  title: string | null;
  source: "session" | "terminal";
  messages: Message[];
};

/* One entry of Herdr's agent list, joined with its workspace label by the backend. */
export type AgentSummary = {
  pane_id: string;
  workspace_id: string;
  workspace_label: string | null;
  tab_id: string;
  agent: string;
  agent_status: AgentStatus | string;
  title: string | null;
  cwd: string | null;
  focused: boolean;
};

export type AgentList = { agents: AgentSummary[] };

const POLL_MS = 2000;
const HIDDEN_POLL_MS = 30_000;

/* The Rust server reports failures as `{ "error": "..." }`. */
async function serverError(response: Response): Promise<string | null> {
  try {
    const body = (await response.json()) as { error?: unknown };
    return typeof body.error === "string" ? body.error : null;
  } catch {
    return null;
  }
}

async function getJson<T>(path: string, mock: (() => Promise<T>) | null): Promise<T> {
  try {
    const response = await fetch(path, { cache: "no-store" });
    if (!response.ok) throw new Error((await serverError(response)) ?? `${path} returned HTTP ${response.status}`);
    return (await response.json()) as T;
  } catch (error) {
    if (mock) return mock();
    // fetch rejects with a TypeError when nothing is listening, e.g. after the idle shutdown.
    if (error instanceof TypeError) throw new Error("the Herdr Lens server is not running; open the conversation from Herdr again");
    throw error;
  }
}

// `vite dev` has no backend, so it falls back to sample data; production builds drop the mock module.
const mockAgents = import.meta.env.DEV ? async () => (await import("./mock")).MOCK_AGENTS : null;
const mockConversation = import.meta.env.DEV ? async () => (await import("./mock")).MOCK_CONVERSATION : null;

export const loadAgents = () => getJson<AgentList>("./api/agents", mockAgents);

export const loadConversation = () => getJson<Conversation>("./api/conversation", mockConversation);

export type Live<T> = { data: T | null; error: string | null };

/*
 * Polls `load` and keeps the last good value on transient errors. Hidden tabs poll slowly
 * instead of stopping, because the server shuts down after a few minutes without requests.
 */
export function useLive<T>(load: () => Promise<T>): Live<T> {
  const [state, setState] = useState<Live<T>>({ data: null, error: null });

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let lastJson = "";

    const tick = async () => {
      clearTimeout(timer);
      try {
        const data = await load();
        // Unchanged payloads keep their object identity so the transcript does not re-render.
        const json = JSON.stringify(data);
        if (!stopped && json !== lastJson) {
          lastJson = json;
          setState({ data, error: null });
        } else if (!stopped) {
          setState((current) => (current.error ? { ...current, error: null } : current));
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (!stopped) setState((current) => ({ data: current.data, error: message }));
      }
      if (!stopped) timer = setTimeout(tick, document.hidden ? HIDDEN_POLL_MS : POLL_MS);
    };

    const onVisible = () => {
      if (!document.hidden) tick();
    };

    tick();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      stopped = true;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [load]);

  return state;
}
