# Herdr Lens agent instructions

## Goal

Build a Herdr plugin that opens the conversation of the agent in the invoking pane in a local browser. Rust owns the Herdr integration and local HTTP server. TypeScript owns the browser UI.

## Repository layout

- `herdr-plugin.toml`: Herdr plugin metadata and the `open` action.
- `herdr-plugin.toml` build step: `sh scripts/install.sh`, which installs the prebuilt release package for `v<version>` (binary to `target/release/`, UI to `web/dist/`) and falls back to `mbx build --release` plus the `web/` bun build. The action runs `target/release/herdr-lens open`.
- `.github/workflows/release-please.yml`: on pushes to `main`, release-please (`release-please-config.json`, `.release-please-manifest.json`) keeps a release PR that bumps `Cargo.toml`, `Cargo.lock`, `herdr-plugin.toml`, and `web/package.json` from Conventional Commits and updates `CHANGELOG.md`; merging it creates the tag and release, then calls `release.yml` to build assets. Do not bump versions by hand.
- `.github/workflows/release.yml`: on `v*` tags or when called with a `tag` input, builds macOS and musl Linux packages (`herdr-lens-<target>.tar.gz` + `.sha256`) and uploads them to the release. The tag must equal the manifest `version`.
- `src/`: Rust backend and action entrypoint (CLI via `usage-rs`, HTTP via `tiny_http`, ANSI via `anstyle-parse`).
  - `main.rs`: `open` and `serve --pane <id>` subcommands.
  - `herdr.rs`: thin wrapper over the Herdr CLI (`pane get`, `agent list`, `workspace list`, `agent read`); all Herdr calls go through it.
  - `context.rs`: resolves the invoking pane from `HERDR_PLUGIN_CONTEXT_JSON`, then `HERDR_PANE_ID`, and requires a detected agent. No focused-pane fallback.
  - `launch.rs`: `open` re-executes the binary as `serve` in its own process group, waits for the first stdout line (`ready <url>` or `error <message>`), then opens the URL with `open`/`xdg-open`.
- `server.rs`: serves `web/dist` and the JSON API on `127.0.0.1` under a random token path, rejects non-loopback `Host` headers, validates selected pane, terminal, and agent identities, and exits after 5 minutes without requests.
  - `load.rs`: builds the `Conversation` for the target, preferring a session reader and falling back to terminal scrollback (`source: terminal`).
  - `conversation.rs`: agent-neutral model (`Conversation`, `Message`, `Role`, `ToolCall`, `Source`) serialized to the UI; keep it in sync with `web/src/api.ts`.
  - `session/<agent>.rs`: readers for an agent's own session log (complete history). `session/droid.rs` reads `~/.factory/sessions/<cwd-slug>/<session-id>.jsonl`, only the id Herdr reports, and checks the log's `session_start` id.
  - `parser/<agent>.rs`: parsers for terminal scrollback, the partial fallback. `parser/mod.rs` falls back to a raw `Terminal` message for unknown agents or unattributable output.
  - `terminal.rs`: decodes ANSI text into `StyledLine`s (SGR only) for the parsers.
- `agents.rs`: sidebar summaries from `herdr agent list` plus workspace labels and terminal IDs; no conversation content.
  - To support a new agent, add `session/<agent>.rs` (preferred) and/or `parser/<agent>.rs`, dispatch it from `load.rs` / `parser/mod.rs`, and add fixtures under `fixtures/<agent>/`.
- `web/`: TypeScript browser UI (React, Tailwind v4, Vite); `web/dist/` is generated output. The UI is read-only: it renders the conversation and never sends input to an agent.
  - `web/src/App.tsx`, `main.tsx`: app shell and polling.
  - `web/src/components/`: `AgentSidebar`, `ConversationPane`, `ActivityRun` (grouped tool calls and reasoning), `Markdown`.
  - `web/src/mock.ts`: synthetic data for `mise x -- bun run dev`; must not reach production builds.
- `web/src/ui/`: primitives adapted from Beautiful UI (https://www.beautifului.dev, MIT, see `web/src/ui/LICENSE`). Beautiful UI has no package; copy and adapt its component source here instead of adding dependencies.
- `web/src/api.ts`: the JSON contract the Rust server must serve: `GET api/agents` (`{ agents: AgentSummary[] }`, from `herdr agent list` plus workspace labels), `GET api/conversation` for the invoking pane, and `GET api/conversation/<pane>/<terminal>/<agent>` for a selected sidebar agent. The UI polls both.
- `fixtures/<agent>/`: real session logs used by backend parser tests, one directory per code agent (e.g. `fixtures/droid/session.jsonl`). The `*.jsonl` files are git-ignored because they contain local conversation content; provide them locally before running `mise x -- mbx test`.

## Development rules

- Follow the installed Herdr CLI and the official plugin documentation when changing the manifest or API calls. The plugin API is out of process; use `HERDR_BIN_PATH` for Herdr CLI calls when possible.
- Resolve the target agent from `HERDR_PLUGIN_CONTEXT_JSON` or the injected pane identifiers. Never silently substitute another focused pane when the target is missing.
- The backend returns the complete history of the selected pane's conversation. Read it from the agent's session log identified by Herdr's `agent_session` for that pane; never open a different session for the pane. Sidebar selections without a supported session reader display `This harness is not supported yet`.
- Keep the web server on loopback behind the token path. Do not expose conversation content on a public interface. Avoid logging full transcripts or secrets, including the viewer URL; server errors go to `HERDR_PLUGIN_STATE_DIR/server.log`.
- The default conversation route stays pinned to the invoking pane. Sidebar selection is allowed only for an agent in Herdr's current agent list whose pane, `terminal_id`, and agent identity still match; `api/agents` returns metadata only.
- Keep durable configuration and runtime state in `HERDR_PLUGIN_CONFIG_DIR` and `HERDR_PLUGIN_STATE_DIR`, not in the plugin checkout.
- Keep Rust responsible for transport, data retrieval, and serving assets. Keep TypeScript responsible for rendering and browser interactions.

## Local checks

- `mise x -- mbx check`, `mise x -- mbx clippy --all-targets`, `mise x -- mbx test`
- `target/debug/herdr-lens serve --pane <pane-id>` prints `ready <url>` and serves that pane without opening a browser.
- `mise x -- bun install --cwd web --frozen-lockfile && mise x -- bun run --cwd web build`
- `mise x -- bun run --cwd web dev` serves the UI on loopback with synthetic data from `web/src/mock.ts` when no backend answers; production builds exclude the mock.
- `hk.pkl` defines the pre-commit hook (`cargo-fmt`, `cargo-clippy`, `web-tsc`) and a commit-msg hook that enforces Conventional Commits (e.g. `feat: add x`); run `mise x -- hk install` once, and `mise x -- hk check --all` to run it manually.
- `mise run build` builds both parts and runs `herdr plugin link .` for local linking; `herdr plugin link .` alone does not run manifest build steps.

## Notes
- Do NOT use the skill `herdr` unless the user has explicitly asked.
