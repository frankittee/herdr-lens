# Herdr Lens

A [Herdr](https://herdr.dev) plugin that opens the conversation of the agent in the invoking pane in a read-only browser view.

- Complete history for supported harnesses, read from the agent's own session log rather than the visible screen.
- Read-only: the viewer renders the conversation and never sends input to an agent.
- Sidebar listing the other agents in Herdr, each pinned to its pane, terminal, and agent identity.
- Markdown with inline `$...$` and display `$$` math; the KaTeX fonts are bundled for offline viewing, and hovering a formula reveals **Copy** for its LaTeX source.
- Loopback-only server behind a random token path.

## Requirements

- Herdr 0.7.0 or newer, on macOS or Linux.
- Prebuilt installs need `sh`, `tar`, and `curl` or an authenticated [`gh`](https://cli.github.com). Prebuilt packages cover macOS (arm64, x86_64) and Linux (arm64, x86_64, static musl).
- Building from source needs [`mise`](https://mise.jdx.dev), which provides Rust, Bun, and mr-boxington (`mbx`). Herdr does not install missing toolchains.

## Install

From GitHub:

```sh
herdr plugin install frankittee/herdr-lens
```

Herdr clones the repository, runs the manifest build step `sh scripts/install.sh`, and links the plugin. The script downloads `herdr-lens-<target>.tar.gz` from the GitHub release `v<version>` matching `herdr-plugin.toml`, verifies its sha256, and unpacks the binary to `target/release/` and the UI to `web/dist/`. If no package fits or the download fails, it builds from source with `mise`. Set `HERDR_LENS_FROM_SOURCE=1` to always build from source, for example when installing an unreleased `--ref`. Pin a revision with `--ref <ref>`, or run non-interactively with `--yes`. The repository is private, so cloning needs configured `git` credentials for GitHub, and downloading releases needs `gh auth login`.

To publish a release, bump `version` in `herdr-plugin.toml`, then push the tag `v<version>`. `.github/workflows/release.yml` checks that the tag matches the manifest and uploads the packages.

From a local checkout:

```sh
git clone git@github.com:frankittee/herdr-lens.git
cd herdr-lens
mise run build
```

`mise run build` builds both parts and runs `herdr plugin link .`. Linking alone does not run the manifest build steps, so build first.

The plugin starts one shared viewer when the Herdr server starts. Restart the Herdr server after installing or updating, since the startup hook does not run when a client attaches or the plugin is linked.

```sh
herdr plugin list                          # confirm the plugin is registered
herdr plugin uninstall herdr-lens          # remove it
```

## Usage

Both actions appear in the pane context menu and pin the invoking pane:

| Action | What it does |
| --- | --- |
| **Open link** | Opens the invoking pane's conversation in the default browser. Use this when Herdr runs on this machine. |
| **Copy link** | Opens a small popup that copies the viewer URL to your clipboard through the terminal (OSC 52) and shows it with the SSH forwarding command. Works over `ssh` and `herdr --remote`, because the copy lands on the machine running your terminal. Press Enter to close. |

The viewer opens on the invoking pane. The sidebar switches to another agent only while that agent is still in Herdr's agent list and its pane, terminal, and agent still match.

### Recommended keybindings

Add both actions to `~/.config/herdr/config.toml` (`%APPDATA%\herdr\config.toml` on Windows) so they are one keystroke away:

```toml
[[keys.command]]
key = "prefix+shift+o"
type = "plugin_action"
command = "herdr-lens.open"
description = "Open link"

[[keys.command]]
key = "prefix+shift+c"
type = "plugin_action"
command = "herdr-lens.copy"
description = "Copy link"
```

Neither key is a Herdr default. Apply the change with `herdr server reload-config` (or **reload config** in Herdr's global menu); keybindings reload without restarting panes. `herdr config check` validates the file first, and each `description` is what `prefix+?` lists.

A keybinding invokes the action on the focused pane, exactly like choosing it from the pane context menu.

### Remote sessions

The viewer binds to `127.0.0.1` on the remote host. Forward the port shown in the URL (for example, `12345`):

```sh
ssh -N -L 12345:127.0.0.1:12345 workbox
```

Then paste the copied URL on this machine. If the local port is occupied, choose another local port and replace only the port in the URL. Copying needs a terminal that accepts OSC 52 clipboard writes (Ghostty, kitty, WezTerm, iTerm2 with the option enabled; inside tmux set `set-clipboard on`); otherwise copy the URL shown in the popup.

## Supported agents

| Agent | Conversation source |
| --- | --- |
| Codex | `~/.codex/sessions/<year>/<month>/<day>/rollout-*-<session-id>.jsonl`, using only the session id Herdr reports for the pane. |
| Droid | `~/.factory/sessions/<cwd-slug>/<session-id>.jsonl`, using only the session id Herdr reports for the pane. |
| Other | Terminal scrollback only, marked as a terminal source. Selecting one of these from the sidebar shows `This harness is not supported yet`. |

For Codex and Droid, the switch in the top-right corner of the viewer chooses between **Session** (the complete history, labeled with the session id) and **Terminal** (the pane's scrollback, which may be incomplete). When Herdr has not reported a session for the pane, for example after `codex resume` on Codex versions that skip the `SessionStart` hook, the viewer falls back to the terminal and disables **Session**.

Every reader verifies that the log it found belongs to the session Herdr reports for that pane; a mismatch is an error rather than another session's transcript.

## Configuration and state

| Variable | Purpose |
| --- | --- |
| `HERDR_PLUGIN_CONFIG_DIR` | Durable plugin configuration. |
| `HERDR_PLUGIN_STATE_DIR` | Runtime state; server errors are appended to `server.log` here. |
| `HERDR_BIN_PATH` | Herdr CLI used for `pane get`, `agent list`, `workspace list`, and `agent read`. |

## Privacy

The server listens on `127.0.0.1` only, rejects non-loopback `Host` headers, and serves everything under a random token path so other local pages cannot read a transcript. Transcripts and the viewer URL are never logged. The viewer exits after five minutes without requests; the shared startup viewer runs while the Herdr server socket exists.

## Development

```sh
mise x -- mbx check
mise x -- mbx clippy --all-targets
mise x -- mbx test
mise x -- bun install --cwd web --frozen-lockfile && mise x -- bun run --cwd web build
```

To inspect a pane without opening a browser, run `target/debug/herdr-lens serve --pane <pane-id>`, which prints `ready <url>`. `mise x -- bun run --cwd web dev` serves the UI with synthetic data when no backend answers; production builds exclude the mock. Architecture, module responsibilities, and the rules for adding an agent live in [AGENTS.md](AGENTS.md).
