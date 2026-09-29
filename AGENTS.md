# Herdr Lens agent instructions

## Goal

Build a Herdr plugin that opens the conversation of the agent in the invoking pane in a local browser. Rust owns the Herdr integration and local HTTP server. TypeScript owns the browser UI.

## Repository layout

- `herdr-plugin.toml`: Herdr plugin metadata and the `open` action.
- `src/`: Rust backend and action entrypoint. `open` validates the invoking pane, starts `herdr-lens serve --pane <id>` as a detached process (`src/launch.rs`), and opens its URL with `open`/`xdg-open`. `src/server.rs` serves `web/dist` and the JSON API on `127.0.0.1` under a random token path, rejects non-loopback `Host` headers, and exits after 5 minutes without requests.
- `web/`: TypeScript browser UI (React, Tailwind v4, Vite); `web/dist/` is generated output. The UI is read-only: it renders the conversation and never sends input to an agent.
- `web/src/ui/`: primitives adapted from Beautiful UI (https://www.beautifului.dev, MIT, see `web/src/ui/LICENSE`). Beautiful UI has no package; copy and adapt its component source here instead of adding dependencies.
- `web/src/api.ts`: the JSON contract the Rust server must serve: `GET api/agents` (`{ agents: AgentSummary[] }`, from `herdr agent list` plus workspace labels) and `GET api/conversation` (the `Conversation` from `src/conversation.rs`). The UI polls both.
- `fixtures/<agent>/`: real session logs used by backend parser tests, one directory per code agent (e.g. `fixtures/droid/session.jsonl`). The `*.jsonl` files are git-ignored because they contain local conversation content; provide them locally before running `cargo test`.

## Development rules

- Follow the installed Herdr CLI and the official plugin documentation when changing the manifest or API calls. The plugin API is out of process; use `HERDR_BIN_PATH` for Herdr CLI calls when possible.
- Resolve the target agent from `HERDR_PLUGIN_CONTEXT_JSON` or the injected pane identifiers. Never silently substitute another focused pane when the target is missing.
- The backend returns the complete history of the invoking pane's conversation only. Read it from the agent's session log identified by Herdr's `agent_session` for that pane; never open or serve any other session. Terminal scrollback is a partial fallback for agents without a session reader.
- Keep the web server on loopback behind the token path. Do not expose conversation content on a public interface. Avoid logging full transcripts or secrets, including the viewer URL; server errors go to `HERDR_PLUGIN_STATE_DIR/server.log`.
- The server is pinned to the invoking pane and its `terminal_id`; `api/agents` returns pane metadata only, never another pane's conversation.
- Keep durable configuration and runtime state in `HERDR_PLUGIN_CONFIG_DIR` and `HERDR_PLUGIN_STATE_DIR`, not in the plugin checkout.
- Keep Rust responsible for transport, data retrieval, and serving assets. Keep TypeScript responsible for rendering and browser interactions.

## Local checks

- `cargo check`, `cargo clippy --all-targets`, `cargo test`
- `target/debug/herdr-lens serve --pane <pane-id>` prints `ready <url>` and serves that pane without opening a browser.
- `bun install --cwd web --frozen-lockfile && bun run --cwd web build`
- `bun run --cwd web dev` serves the UI on loopback with synthetic data from `web/src/mock.ts` when no backend answers; production builds exclude the mock.
- For local linking, build both parts first; `herdr plugin link .` does not run manifest build steps.

## Notes
- Do NOT use the skill `herdr` unless the user has explicitly asked.
