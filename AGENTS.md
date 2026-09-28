# Herdr Lens agent instructions

## Goal

Build a Herdr plugin that opens the conversation of the agent in the invoking pane in a local browser. Rust owns the Herdr integration and local HTTP server. TypeScript owns the browser UI.

## Repository layout

- `herdr-plugin.toml`: Herdr plugin metadata and the `open` action.
- `src/`: Rust backend and action entrypoint.
- `web/`: TypeScript browser UI; `web/dist/` is generated output.
- `fixtures/<agent>/`: real session logs used by backend parser tests, one directory per code agent (e.g. `fixtures/droid/session.jsonl`). The `*.jsonl` files are git-ignored because they contain local conversation content; provide them locally before running `cargo test`.

## Development rules

- Follow the installed Herdr CLI and the official plugin documentation when changing the manifest or API calls. The plugin API is out of process; use `HERDR_BIN_PATH` for Herdr CLI calls when possible.
- Resolve the target agent from `HERDR_PLUGIN_CONTEXT_JSON` or the injected pane identifiers. Never silently substitute another focused pane when the target is missing.
- The backend returns the complete history of the invoking pane's conversation only. Read it from the agent's session log identified by Herdr's `agent_session` for that pane; never open or serve any other session. Terminal scrollback is a partial fallback for agents without a session reader.
- Bind the future web server to loopback by default. Do not expose conversation content on a public interface. Avoid logging full transcripts or secrets.
- Keep durable configuration and runtime state in `HERDR_PLUGIN_CONFIG_DIR` and `HERDR_PLUGIN_STATE_DIR`, not in the plugin checkout.
- Keep Rust responsible for transport, data retrieval, and serving assets. Keep TypeScript responsible for rendering and browser interactions.
- Do not claim the viewer works until the Rust action opens a browser and serves the selected conversation. The current entrypoint and UI are scaffolds.

## Local checks

- `cargo check`
- `bun install --cwd web --frozen-lockfile && bun run --cwd web build`
- For local linking, build both parts first; `herdr plugin link .` does not run manifest build steps.

## Notes
- Do NOT use the skill `herdr` unless the user has explicitly asked.
