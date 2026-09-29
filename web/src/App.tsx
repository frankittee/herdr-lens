import { useCallback, useEffect, useState } from "react";
import { loadAgents, loadConversation, useLive } from "./api";
import { AgentSidebar } from "./components/AgentSidebar";
import { ConversationPane } from "./components/ConversationPane";

/* Below this width the expanded sidebar would squeeze the transcript, so it overlays instead. */
const NARROW_QUERY = "(max-width: 960px)";

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
  const conversation = useLive(loadConversation);
  const narrow = useMediaQuery(NARROW_QUERY);
  const [collapsed, setCollapsed] = useState(narrow);

  // Crossing the breakpoint resets the sidebar to the default for the new width.
  useEffect(() => setCollapsed(narrow), [narrow]);

  const toggle = useCallback(() => setCollapsed((current) => !current), []);

  return (
    <div className="flex h-full overflow-hidden">
      <AgentSidebar
        agents={agents.data?.agents ?? null}
        currentPaneId={conversation.data?.pane_id ?? null}
        error={agents.error}
        collapsed={collapsed}
        overlay={narrow}
        onToggle={toggle}
      />
      <ConversationPane conversation={conversation.data} error={conversation.error} />
    </div>
  );
}
