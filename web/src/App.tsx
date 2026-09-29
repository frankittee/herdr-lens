import { useCallback, useEffect, useState } from "react";
import { loadAgents, loadConversation, useLive, type AgentSummary, type Source } from "./api";
import { AgentSidebar } from "./components/AgentSidebar";
import { ConversationPane } from "./components/ConversationPane";

/* Below this width the expanded sidebar would squeeze the transcript, so it overlays instead. */
const NARROW_QUERY = "(max-width: 960px)";
const PANE_SLUG = /^w[A-Za-z0-9]+-p[A-Za-z0-9]+$/;

function paneFromUrl(): string | null {
  const segment = window.location.pathname.split("/").filter(Boolean).at(-1);
  return segment && PANE_SLUG.test(segment) ? segment.replace("-", ":") : null;
}

function setPaneUrl(paneId: string) {
  const segments = window.location.pathname.split("/").filter(Boolean);
  if (PANE_SLUG.test(segments.at(-1) ?? "")) segments.pop();
  segments.push(paneId.replace(":", "-"));
  window.history.pushState(null, "", `/${segments.join("/")}/`);
}

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const update = () => setMatches(media.matches);
    // `resize` is a fallback: some embedded and emulated viewports drop media `change` events.
    media.addEventListener("change", update);
    window.addEventListener("resize", update);
    return () => {
      media.removeEventListener("change", update);
      window.removeEventListener("resize", update);
    };
  }, [query]);
  return matches;
}

export function App() {
  const agents = useLive(loadAgents);
  const narrow = useMediaQuery(NARROW_QUERY);
  const [collapsed, setCollapsed] = useState(narrow);
  const [selected, setSelected] = useState<AgentSummary | null>(null);
  const supported = selected?.supported ?? true;
  const [source, setSource] = useState<Source>("session");
  const agentKey = selected ? `${selected.pane_id}/${selected.terminal_id}/${selected.agent}` : "invoking-pane";
  const selectedKey = `${agentKey}/${source}`;
  const load = useCallback(
    () => loadConversation(selected?.pane_id, selected?.terminal_id ?? undefined, selected?.agent, source),
    [selected?.pane_id, selected?.terminal_id, selected?.agent, source],
  );
  const conversation = useLive(load, selectedKey, supported);

  useEffect(() => {
    const syncFromUrl = () => {
      const paneId = paneFromUrl();
      if (!paneId) return;
      const agent = agents.data?.agents.find((item) => item.pane_id === paneId && item.terminal_id);
      if (agent) setSelected(agent);
    };
    syncFromUrl();
    window.addEventListener("popstate", syncFromUrl);
    return () => window.removeEventListener("popstate", syncFromUrl);
  }, [agents.data]);

  // Crossing the breakpoint resets the sidebar to the default for the new width.
  useEffect(() => setCollapsed(narrow), [narrow]);

  const toggle = useCallback(() => setCollapsed((current) => !current), []);
  const select = useCallback((agent: AgentSummary) => {
    if (!agent.terminal_id) return;
    setSelected(agent);
    if (paneFromUrl() !== agent.pane_id) setPaneUrl(agent.pane_id);
    if (narrow) setCollapsed(true);
  }, [narrow]);

  return (
    <div className="flex h-full overflow-hidden">
      <AgentSidebar
        agents={agents.data?.agents ?? null}
        currentPaneId={selected?.pane_id ?? conversation.data?.pane_id ?? null}
        error={agents.error}
        collapsed={collapsed}
        overlay={narrow}
        onToggle={toggle}
        onSelect={select}
      />
      <ConversationPane
        conversation={conversation.data}
        error={supported ? conversation.error : "This harness is not supported yet"}
        onSource={setSource}
      />
    </div>
  );
}
