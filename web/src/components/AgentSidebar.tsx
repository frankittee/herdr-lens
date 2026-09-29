import { useEffect } from "react";
import type { AgentSummary } from "../api";
import { EyeIcon, SidebarIcon } from "../ui/icons";
import { STATUS_LABELS, StatusMark, StatusPill } from "../ui/status";

const FULL_WIDTH = 300;
const RAIL_WIDTH = 52;

type Group = { id: string; label: string; agents: AgentSummary[] };

function groupByWorkspace(agents: AgentSummary[]): Group[] {
  const groups = new Map<string, Group>();
  for (const agent of agents) {
    const id = agent.workspace_id;
    const group = groups.get(id) ?? { id, label: agent.workspace_label || id, agents: [] };
    group.agents.push(agent);
    groups.set(id, group);
  }
  return [...groups.values()];
}

function describe(agent: AgentSummary, workspace: string): string {
  const status = STATUS_LABELS[agent.agent_status] ?? agent.agent_status;
  return [`${agent.agent} (${agent.pane_id})`, agent.title, `${workspace} · ${status}`].filter(Boolean).join("\n");
}

function AgentRow({ agent, workspace, current }: { agent: AgentSummary; workspace: string; current: boolean }) {
  return (
    <li
      aria-current={current ? "true" : undefined}
      title={describe(agent, workspace)}
      className={`sidebar-row relative flex items-center gap-2.5 overflow-hidden border-b border-line py-2 pr-2.5 pl-1.5 last:border-b-0 ${
        current ? "bg-accent-tint" : ""
      }`}
    >
      {current && <span aria-hidden className="sidebar-copy absolute inset-y-1.5 left-0 w-[3px] rounded-r-full bg-accent" />}
      <span className="flex size-6 shrink-0 items-center justify-center">
        <StatusMark status={agent.agent_status} />
      </span>
      <span className="sidebar-copy flex min-w-0 flex-1 flex-col">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-[13px] font-medium text-ink">{agent.agent}</span>
          <span className="shrink-0 font-mono text-[11px] text-ink-3">{agent.pane_id}</span>
        </span>
        <span className="truncate text-[12px] text-ink-2">{agent.title || agent.cwd || "No title"}</span>
      </span>
      <span className="sidebar-copy flex shrink-0">
        <StatusPill status={agent.agent_status} />
      </span>
    </li>
  );
}

function ToggleButton({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      aria-expanded={!collapsed}
      onClick={onToggle}
      className={`absolute top-2 flex size-8 items-center justify-center rounded-[8px] text-ink-3 transition-colors duration-150 hover:bg-hover-2 hover:text-ink ${
        collapsed ? "left-2.5" : "right-2"
      }`}
    >
      <SidebarIcon size={17} />
    </button>
  );
}

/*
 * Wide windows: the sidebar sits in the layout and collapses to an icon rail.
 * Narrow windows: only the rail takes layout space; expanding overlays the conversation.
 */
export function AgentSidebar({
  agents,
  currentPaneId,
  error,
  collapsed,
  overlay,
  onToggle,
}: {
  agents: AgentSummary[] | null;
  currentPaneId: string | null;
  error: string | null;
  collapsed: boolean;
  overlay: boolean;
  onToggle: () => void;
}) {
  const groups = agents ? groupByWorkspace(agents) : [];
  const working = agents?.filter((agent) => agent.agent_status === "working").length ?? 0;
  const floating = overlay && !collapsed;

  useEffect(() => {
    if (!floating) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onToggle();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [floating, onToggle]);

  return (
    <div
      className="relative h-full shrink-0 transition-[width] duration-[280ms]"
      style={{ width: collapsed || overlay ? RAIL_WIDTH : FULL_WIDTH, transitionTimingFunction: "cubic-bezier(0.16, 1, 0.3, 1)" }}
    >
      {floating && (
        <div
          aria-hidden
          onClick={onToggle}
          className="fixed inset-0 z-20 bg-black/25"
          style={{ animation: "fade-in 200ms ease-out both" }}
        />
      )}
      <aside
        data-collapsed={collapsed}
        aria-label="Agents"
        className="sidebar absolute inset-y-0 left-0 z-30 flex overflow-hidden border-r border-line bg-canvas"
        style={{
          width: collapsed ? RAIL_WIDTH : FULL_WIDTH,
          boxShadow: floating ? "var(--shadow-overlay)" : "none",
        }}
      >
        <div className="flex h-full shrink-0 flex-col" style={{ width: FULL_WIDTH }}>
          <div className="relative h-12 shrink-0 border-b border-line">
            <div className="sidebar-copy flex h-full items-center gap-2 pr-12 pl-4">
              <span className="text-[14px] font-semibold tracking-tight text-ink">Herdr Lens</span>
              <span className="inline-flex items-center gap-1 rounded-full bg-field px-2 py-0.5 text-[11px] font-medium text-ink-2 shadow-hairline">
                <EyeIcon size={11} />
                Read-only
              </span>
            </div>
            <ToggleButton collapsed={collapsed} onToggle={onToggle} />
          </div>

          <div className="sidebar-copy flex items-baseline justify-between px-4 pt-4 pb-2">
            <span className="text-[12px] font-medium tracking-wide text-ink-2 uppercase">Agents</span>
            {agents && (
              <span className="text-[12px] text-ink-3 tabular-nums">
                {working} working · {agents.length} total
              </span>
            )}
          </div>

          <div
            className={`flex min-h-0 flex-1 flex-col gap-4 overflow-x-hidden px-2 pb-4 ${collapsed ? "overflow-y-hidden hover:overflow-y-auto" : "overflow-y-auto"}`}
          >
            {groups.map((group, i) => (
              <section key={group.id} style={{ animation: `fade-up 450ms var(--ease-out-strong) ${i * 80}ms both` }}>
                <h2 className="sidebar-copy mb-1.5 truncate px-2 text-[12px] font-medium text-ink-2">{group.label}</h2>
                <ul className="sidebar-card overflow-hidden rounded-card bg-surface shadow-card">
                  {group.agents.map((agent) => (
                    <AgentRow
                      key={agent.pane_id}
                      agent={agent}
                      workspace={group.label}
                      current={agent.pane_id === currentPaneId}
                    />
                  ))}
                </ul>
              </section>
            ))}
            <div className="sidebar-copy px-2 text-[12.5px]">
              {agents && agents.length === 0 && <p className="text-ink-3">No agents detected in Herdr.</p>}
              {!agents && !error && <p className="text-ink-3">Loading agents…</p>}
              {error && <p className="text-[12px] text-red">Could not load agents: {error}</p>}
            </div>
          </div>
        </div>
      </aside>
    </div>
  );
}
