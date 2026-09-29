import Amp from "@lobehub/icons/es/Amp/components/Mono";
import Antigravity from "@lobehub/icons/es/Antigravity/components/Mono";
import ClaudeCode from "@lobehub/icons/es/ClaudeCode/components/Mono";
import Cline from "@lobehub/icons/es/Cline/components/Mono";
import Codex from "@lobehub/icons/es/Codex/components/Mono";
import Cursor from "@lobehub/icons/es/Cursor/components/Mono";
import Devin from "@lobehub/icons/es/Devin/components/Mono";
import GeminiCLI from "@lobehub/icons/es/GeminiCLI/components/Mono";
import GithubCopilot from "@lobehub/icons/es/GithubCopilot/components/Mono";
import Grok from "@lobehub/icons/es/Grok/components/Mono";
import HermesAgent from "@lobehub/icons/es/HermesAgent/components/Mono";
import KiloCode from "@lobehub/icons/es/KiloCode/components/Mono";
import Kimi from "@lobehub/icons/es/Kimi/components/Mono";
import Kiro from "@lobehub/icons/es/Kiro/components/Mono";
import Mastra from "@lobehub/icons/es/Mastra/components/Mono";
import OpenCode from "@lobehub/icons/es/OpenCode/components/Mono";
import Pi from "@lobehub/icons/es/Pi/components/Mono";
import Qoder from "@lobehub/icons/es/Qoder/components/Mono";
import Qwen from "@lobehub/icons/es/Qwen/components/Mono";
import droidIcon from "../assets/droid-zed.svg";
import { ToolIcon } from "../ui/icons";
import { SpinnerRing } from "../ui/status";

const ICONS = {
  agy: Antigravity,
  amp: Amp,
  claude: ClaudeCode,
  cline: Cline,
  codex: Codex,
  copilot: GithubCopilot,
  cursor: Cursor,
  devin: Devin,
  gemini: GeminiCLI,
  grok: Grok,
  hermes: HermesAgent,
  kilo: KiloCode,
  kimi: Kimi,
  kiro: Kiro,
  mastracode: Mastra,
  omp: Pi,
  opencode: OpenCode,
  pi: Pi,
  qodercli: Qoder,
  qwen: Qwen,
} as const;

export function AgentIcon({ agent, working }: { agent: string; working: boolean }) {
  const Icon = ICONS[agent.toLowerCase() as keyof typeof ICONS];
  return (
    <span aria-hidden className="relative flex size-8 shrink-0 items-center justify-center">
      {working && <span className="absolute inset-0"><SpinnerRing active size={32} /></span>}
      <span className={`relative flex size-6 items-center justify-center overflow-hidden rounded-full shadow-hairline ${agent === "droid" ? "bg-[#1f2530]" : "bg-field text-ink"}`}>
        {agent === "droid" ? <img src={droidIcon} alt="" className="size-4.5" /> : Icon ? <Icon size={16} /> : <ToolIcon size={15} />}
      </span>
    </span>
  );
}
