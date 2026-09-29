//! Reads Codex's rollout session log, one JSONL file per session under
//! `~/.codex/sessions/<year>/<month>/<day>/rollout-*-<session-id>.jsonl`.
//!
//! Only the session Herdr reports for the target pane is opened: `id` references
//! are looked up under `~/.codex/sessions`, `path` references must name a
//! `rollout-*-<session-id>.jsonl` file, and the log's own `session_meta` id must
//! match in both cases. The timeline comes from `response_item` records;
//! `event_msg` items repeat the same content and are skipped.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::conversation::{Message, Role, Source, ToolCall, Transcript};
use crate::herdr::AgentSession;

/// Pseudo-tool name Codex reports for shell commands run through its JS bridge.
const EXEC_TOOL: &str = "exec";
/// User message wrapper for a shell command the user ran themselves; shown, not skipped.
const SHELL_COMMAND_PREFIX: &str = "<user_shell_command>";

pub fn load(pane_id: &str, session: Option<&AgentSession>) -> Result<Transcript> {
    let session = session.with_context(|| {
        format!("Herdr has not reported a Codex session for pane {pane_id} yet")
    })?;
    ensure!(
        session.agent == "codex",
        "pane {pane_id} reports a {} session, not a Codex session",
        session.agent
    );
    let expected_id: Option<String>;
    let path = match session.kind.as_str() {
        "id" => {
            expected_id = Some(session.value.clone());
            find_by_id(&sessions_root()?, &session.value)?
        }
        "path" => {
            let path = PathBuf::from(&session.value);
            expected_id = Some(
                session_id_from_path(&path)
                    .map(str::to_owned)
                    .with_context(|| {
                        format!("unsupported Codex session path {}", path.display())
                    })?,
            );
            path
        }
        other => bail!("unsupported Codex session reference kind {other:?}"),
    };
    let jsonl = fs::read_to_string(&path)
        .with_context(|| format!("failed to read Codex session log {}", path.display()))?;
    parse(&jsonl, expected_id.as_deref())
}

fn codex_root() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".codex"))
}

fn sessions_root() -> Result<PathBuf> {
    Ok(codex_root()?.join("sessions"))
}

/// Session ids are unique, so the file is looked up in every dated directory.
/// Also rejects path separators, so the id cannot escape the sessions directory.
fn find_by_id(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(is_session_id(id), "invalid Codex session id {id:?}");
    let suffix = format!("-{id}.jsonl");
    let mut matches = Vec::new();
    collect_matching(root, &suffix, &mut matches)?;
    matches.sort();
    matches.into_iter().next().with_context(|| {
        format!(
            "Codex session log rollout-*-{id}.jsonl not found under {}",
            root.display()
        )
    })
}

fn collect_matching(dir: &Path, suffix: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_matching(&path, suffix, out)?;
        } else if path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.starts_with("rollout-") && name.ends_with(suffix)
        }) {
            out.push(path);
        }
    }
    Ok(())
}

/// Session id embedded in a `rollout-*-<session-id>.jsonl` file name, so a Herdr
/// `path` reference cannot point at another session's log without detection.
fn session_id_from_path(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    if is_rollout_id(rest) {
        return Some(rest);
    }
    // rollout-<timestamp>-<session-id>: the id is the trailing UUID.
    let len = rest.len();
    let head = rest.get(..len.checked_sub(36)?)?;
    let tail = rest.get(len - 36..)?;
    (head.ends_with('-') && is_rollout_id(tail)).then_some(tail)
}

/// Rollout file names end with a UUID; stricter than `is_session_id` so a bare
/// timestamp like `rollout-2026-09-28.jsonl` is never mistaken for an id.
fn is_rollout_id(id: &str) -> bool {
    let id = id.as_bytes();
    id.len() == 36
        && id[8] == b'-'
        && id[13] == b'-'
        && id[18] == b'-'
        && id[23] == b'-'
        && id.iter().all(|c| c.is_ascii_hexdigit() || *c == b'-')
}

/// Also rejects path separators, so the id cannot escape the sessions directory.
fn is_session_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry {
    SessionMeta {
        payload: SessionMeta,
    },
    ResponseItem {
        timestamp: Option<String>,
        payload: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct SessionMeta {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

pub fn parse(jsonl: &str, expected_id: Option<&str>) -> Result<Transcript> {
    let lines: Vec<(usize, &str)> = jsonl
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .collect();
    let mut builder = Builder::default();
    let mut started = false;

    for (i, &(number, line)) in lines.iter().enumerate() {
        let entry = match serde_json::from_str::<Entry>(line) {
            Ok(entry) => entry,
            // Codex appends while it works, so the last line may be partially written.
            Err(_) if i + 1 == lines.len() => break,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("malformed Codex session log line {}", number + 1))
            }
        };
        match entry {
            Entry::SessionMeta { payload } => {
                let id = payload
                    .session_id
                    .as_deref()
                    .or(payload.id.as_deref())
                    .unwrap_or("");
                if let Some(expected) = expected_id {
                    ensure!(
                        id == expected,
                        "Codex session log belongs to {id}, not {expected}"
                    );
                }
                started = true;
            }
            Entry::ResponseItem { timestamp, payload } => builder.add(payload, timestamp),
            Entry::Other => {}
        }
    }

    ensure!(started, "not a Codex session log: no session_meta entry");
    Ok(Transcript {
        source: Source::Session,
        title: expected_id.and_then(thread_title),
        messages: builder.messages,
    })
}

/// Session titles live in `~/.codex/session_index.jsonl`, not in the rollout log.
fn thread_title(id: &str) -> Option<String> {
    let index = codex_root().ok()?.join("session_index.jsonl");
    thread_title_in(&index, id)
}

fn thread_title_in(index: &Path, id: &str) -> Option<String> {
    let text = fs::read_to_string(index).ok()?;
    text.lines().find_map(|line| {
        let entry: Value = serde_json::from_str(line).ok()?;
        if entry.get("id")?.as_str()? != id {
            return None;
        }
        entry.get("thread_name")?.as_str().map(str::to_owned)
    })
}

#[derive(Default)]
struct Builder {
    messages: Vec<Message>,
    /// Tool `call_id` -> index of its message, so outputs can be attached to their call.
    tool_calls: HashMap<String, usize>,
}

impl Builder {
    fn add(&mut self, payload: Value, timestamp: Option<String>) {
        match payload.get("type").and_then(Value::as_str) {
            Some("message") => self.add_message(&payload, &timestamp),
            Some("reasoning") => {
                let text = join_part_texts(&payload, "summary");
                if !text.trim().is_empty() {
                    self.push(Message::new(Role::Reasoning, text), &timestamp);
                }
            }
            Some("custom_tool_call") => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let input = payload.get("input").and_then(Value::as_str).unwrap_or("");
                let (title, detail) = if name == EXEC_TOOL {
                    match extract_cmd(input) {
                        Some(cmd) => (format!("{name} {cmd}"), serde_json::json!({"cmd": cmd})),
                        None => (name.to_owned(), Value::String(input.to_owned())),
                    }
                } else {
                    (name.to_owned(), Value::String(input.to_owned()))
                };
                self.add_tool_call(&payload, &title, name, detail, &timestamp);
            }
            Some("function_call") => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let namespace = payload
                    .get("namespace")
                    .and_then(Value::as_str)
                    .map(|ns| ns.strip_prefix("mcp__").unwrap_or(ns))
                    .filter(|ns| *ns != name);
                let title = match namespace {
                    Some(ns) => format!("{ns} {name}"),
                    None => name.to_owned(),
                };
                let args = payload
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let detail: Value =
                    serde_json::from_str(args).unwrap_or_else(|_| Value::String(args.to_owned()));
                self.add_tool_call(&payload, &title, name, detail, &timestamp);
            }
            Some("custom_tool_call_output") | Some("function_call_output") => {
                let call_id = payload.get("call_id").and_then(Value::as_str).unwrap_or("");
                let mut text = join_part_texts(&payload, "output");
                if payload.get("type").and_then(Value::as_str) == Some("custom_tool_call_output") {
                    text = strip_exec_header(&text).to_owned();
                }
                self.attach_result(call_id, &text, &timestamp);
            }
            _ => {}
        }
    }

    /// User and assistant messages. Shell results and injected context
    /// (`<environment_context>`, AGENTS.md instructions, skill wrappers) are handled
    /// separately; anything else is shown, including user XML/HTML.
    fn add_message(&mut self, payload: &Value, timestamp: &Option<String>) {
        let parts = payload
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match payload.get("role").and_then(Value::as_str) {
            Some("user") => {
                let mut text = String::new();
                for part in &parts {
                    let part_text = part.get("text").and_then(Value::as_str).unwrap_or("");
                    if is_shell_command(part_text) {
                        if let Some(shell) = format_shell_command(part_text) {
                            push_block(&mut text, &shell);
                        }
                    } else if is_awareness_context(part_text) {
                        // Injected surroundings (<environment_context>, skill/AGENTS.md
                        // wrappers): never user-typed content, so skipped.
                    } else if !part_text.trim().is_empty() {
                        push_block(&mut text, part_text.trim());
                    }
                }
                if !text.trim().is_empty() {
                    self.push(Message::new(Role::User, text), timestamp);
                }
            }
            Some("assistant") => {
                let text = join_part_texts(payload, "content");
                if !text.trim().is_empty() {
                    self.push(Message::new(Role::Assistant, text), timestamp);
                }
            }
            _ => {}
        }
    }

    fn add_tool_call(
        &mut self,
        payload: &Value,
        title: &str,
        name: &str,
        input: Value,
        timestamp: &Option<String>,
    ) {
        let mut message = Message::tool(title);
        message.tool = Some(ToolCall {
            name: name.to_owned(),
            input,
            pending: true,
            is_error: false,
        });
        if let Some(call_id) = payload.get("call_id").and_then(Value::as_str) {
            self.tool_calls
                .insert(call_id.to_owned(), self.messages.len());
        }
        self.push(message, timestamp);
    }

    fn push(&mut self, mut message: Message, timestamp: &Option<String>) {
        message.timestamp = timestamp.clone();
        self.messages.push(message);
    }

    fn attach_result(&mut self, call_id: &str, text: &str, ts: &Option<String>) {
        let Some(&index) = self.tool_calls.get(call_id) else {
            // Keep outputs whose call is missing from the log rather than dropping them.
            self.push(Message::new(Role::Tool, text), ts);
            return;
        };
        let message = &mut self.messages[index];
        message.text = text.to_owned();
        if let Some(tool) = message.tool.as_mut() {
            tool.pending = false;
        }
    }
}

/// Text of every content part that carries one, joined in order.
fn join_part_texts(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Injected surroundings, not user-typed content: the harness wraps context in
/// known tags, which always arrive as their own message part in the logs seen.
/// A bare `is_awareness_context` check on `<` alone would drop user XML/HTML.
fn is_awareness_context(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<skill>")
        || trimmed.starts_with("# AGENTS.md")
}

fn is_shell_command(text: &str) -> bool {
    text.trim_start().starts_with(SHELL_COMMAND_PREFIX)
}

fn push_block(buffer: &mut String, block: &str) {
    if !buffer.is_empty() {
        buffer.push_str("\n\n");
    }
    buffer.push_str(block);
}

/// Renders a user-run shell command as `$ <command>` followed by its output.
fn format_shell_command(text: &str) -> Option<String> {
    let text = text.trim().strip_prefix(SHELL_COMMAND_PREFIX)?.trim();
    let command = between(text, "<command>", "</command>")?.trim();
    let output = text
        .split("Output:")
        .nth(1)?
        .split("</result>")
        .next()
        .unwrap_or("")
        .trim();
    Some(format!("$ {command}\n{output}"))
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(start)?;
    let (middle, _) = rest.split_once(end)?;
    Some(middle)
}

/// Extracts the `cmd:"..."` argument from an exec JS snippet, handling escapes.
fn extract_cmd(js: &str) -> Option<String> {
    let (_, rest) = js.split_once("cmd:")?;
    let rest = rest.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let mut out = String::new();
    let mut escaped = false;
    for c in rest[quote.len_utf8()..].chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

/// Shell results start with a `Script completed / Wall time / Output:` header.
fn strip_exec_header(text: &str) -> &str {
    let trimmed = text.trim_start();
    match trimmed.strip_prefix("Script completed\n") {
        Some(rest) => match rest.split_once("Output:\n") {
            Some((_, output)) => output.trim(),
            None => text,
        },
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::AgentSession;

    const ID: &str = "01a0e75f-52b8-7820-9231-009c55ccf2c1";

    fn meta() -> String {
        format!(r#"{{"type":"session_meta","payload":{{"id":"{ID}","session_id":"{ID}"}}}}"#)
    }

    fn item(payload: &str) -> String {
        format!(r#"{{"type":"response_item","timestamp":"t","payload":{payload}}}"#)
    }

    fn sample() -> String {
        [
            meta(),
            item(r#"{"type":"message","role":"developer","content":[{"type":"input_text","text":"<skills_instructions>"}]}"#),
            item(r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\nbla"}]}"#),
            item(r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix the parser"}]}"#),
            item(r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"<note>Keep <b>my</b> markup</note>"}]}"#),
            item(r#"{"type":"message","role":"user","content":[{"type":"input_text","text":"<user_shell_command>\n<command>\necho hi\n</command>\n<result>\nExit code: 0\nOutput:\nhi\n</result>\n</user_shell_command>"}]}"#),
            item(r#"{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Checking now."}]}"#),
            item(r#"{"type":"reasoning","summary":[],"encrypted_content":"x"}"#),
            item(r#"{"type":"reasoning","summary":[{"type":"summary_text","text":"Look first."}]}"#),
            item(r#"{"type":"custom_tool_call","call_id":"c1","name":"exec","input":"await tools.exec_command({cmd:\"cargo test\"})"}"#),
            item(r#"{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"input_text","text":"Script completed\nWall time 0.1 seconds\nOutput:\n"},{"type":"input_text","text":"ok"}]}"#),
            item(r#"{"type":"function_call","call_id":"c2","name":"js","namespace":"mcp__cua_repl","arguments":"{\"code\":\"await cua.getState()\"}"}"#),
            item(r#"{"type":"function_call_output","call_id":"c2","output":[{"type":"input_text","text":"state"}]}"#),
            item(r#"{"type":"custom_tool_call","call_id":"c3","name":"exec","input":"await tools.exec_command({cmd:\"sleep 60\"})"}"#),
            // Codex appends while it works, so the last line may be partially written.
            r#"{"type":"response_item","timestamp":"t","payload":{"type":"message""#.to_owned(),
        ]
        .join("\n")
    }

    #[test]
    fn parses_visible_history_and_pairs_tool_results() {
        let transcript = parse(&sample(), Some(ID)).unwrap();
        assert_eq!(transcript.source, Source::Session);

        let m = &transcript.messages;
        let roles: Vec<Role> = m.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [
                Role::User,
                Role::User,
                Role::User,
                Role::Assistant,
                Role::Reasoning,
                Role::Tool,
                Role::Tool,
                Role::Tool,
            ]
        );
        assert_eq!(m[0].text, "Fix the parser");
        assert_eq!(m[1].text, "<note>Keep <b>my</b> markup</note>");
        assert_eq!(m[2].text, "$ echo hi\nhi");
        assert_eq!(m[3].text, "Checking now.");
        assert_eq!(m[4].text, "Look first.");

        assert_eq!(m[5].title.as_deref(), Some("exec cargo test"));
        assert_eq!(m[5].text, "ok");
        let exec = m[5].tool.as_ref().unwrap();
        assert_eq!(exec.name, "exec");
        assert_eq!(exec.input["cmd"], "cargo test");
        assert!(!exec.pending);

        assert_eq!(m[6].title.as_deref(), Some("cua_repl js"));
        assert_eq!(m[6].text, "state");
        assert!(!m[6].tool.as_ref().unwrap().pending);

        assert_eq!(m[7].title.as_deref(), Some("exec sleep 60"));
        assert!(m[7].tool.as_ref().unwrap().pending);
    }

    #[test]
    fn keeps_user_markup_and_skips_only_known_wrappers() {
        assert!(is_awareness_context("<environment_context>\nbla"));
        assert!(is_awareness_context("<skill>\n<name>herdr</name>"));
        assert!(is_awareness_context("# AGENTS.md instructions\n..."));
        for kept in [
            "<note>Keep <b>my</b> markup</note>",
            "<div class=\"x\">hi</div>",
            "<3 love",
            "a < b",
        ] {
            assert!(!is_awareness_context(kept), "dropped user text {kept:?}");
        }
    }

    /// A real Codex session log. Its id and message mix depend on the local fixture.
    #[test]
    fn parses_real_codex_session_log() {
        let jsonl = include_str!("../../fixtures/codex/session.jsonl");
        let transcript = parse(jsonl, None).unwrap();
        assert_eq!(transcript.source, Source::Session);
        assert!(!transcript.messages.is_empty());
    }

    #[test]
    fn reads_thread_title_from_session_index() {
        let dir = env::temp_dir().join(format!("herdr-lens-codex-index-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let index = dir.join("session_index.jsonl");
        fs::write(
            &index,
            format!(
                "{{\"id\":\"other\",\"thread_name\":\"Other\"}}\n{{\"id\":\"{ID}\",\"thread_name\":\"Fix it\"}}\n"
            ),
        )
        .unwrap();

        assert_eq!(thread_title_in(&index, ID).as_deref(), Some("Fix it"));
        assert_eq!(thread_title_in(&index, "missing"), None);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extracts_quoted_commands() {
        assert_eq!(
            extract_cmd(r#"x({cmd:"rg -n 'a b'"})"#).as_deref(),
            Some("rg -n 'a b'")
        );
        assert_eq!(
            extract_cmd("x({cmd:'echo \"hi\"'})").as_deref(),
            Some("echo \"hi\"")
        );
        assert_eq!(extract_cmd("no cmd here"), None);
    }

    #[test]
    fn strips_only_exec_headers() {
        assert_eq!(
            strip_exec_header("Script completed\nWall time 0.1 seconds\nOutput:\nok\n"),
            "ok"
        );
        let plain = "grep Output:\nmore";
        assert_eq!(strip_exec_header(plain), plain);
    }

    #[test]
    fn extracts_session_id_from_rollout_paths() {
        let root = Path::new("/tmp");
        assert_eq!(
            session_id_from_path(&root.join(format!("rollout-2026-09-28T16-37-48-{ID}.jsonl"))),
            Some(ID)
        );
        assert!(session_id_from_path(Path::new("notes.jsonl")).is_none());
        assert!(session_id_from_path(Path::new("session.jsonl")).is_none());
        assert!(session_id_from_path(Path::new("rollout-notes.txt")).is_none());
        assert!(session_id_from_path(Path::new("rollout-2026-09-28.jsonl")).is_none());
    }

    #[test]
    fn rejects_path_pointing_at_another_session() {
        let dir = env::temp_dir().join(format!("herdr-lens-codex-path-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let other = format!("rollout-2026-09-28T16-37-48-{ID}.jsonl");
        fs::write(
            dir.join(&other),
            meta().replace(ID, "00000000-0000-0000-0000-000000000000"),
        )
        .unwrap();

        // A path whose file name disagrees with the log's own session_meta id
        // is caught and never served.
        let session = AgentSession {
            agent: "codex".into(),
            kind: "path".into(),
            source: "herdr:codex".into(),
            value: dir.join(&other).to_string_lossy().into_owned(),
        };
        let err = load("w:p1", Some(&session)).unwrap_err();
        assert!(err.to_string().contains("belongs to"), "{err:?}");

        // A path that does not name a rollout session log is rejected outright.
        let plain = dir.join("notes.jsonl");
        fs::write(&plain, meta()).unwrap();
        let session = AgentSession {
            agent: "codex".into(),
            kind: "path".into(),
            source: "herdr:codex".into(),
            value: plain.to_string_lossy().into_owned(),
        };
        assert!(load("w:p1", Some(&session)).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_log_of_another_session() {
        let err = parse(&sample(), Some("0000")).unwrap_err();
        assert!(err.to_string().contains("belongs to"));
    }

    #[test]
    fn rejects_malformed_middle_line_and_missing_start() {
        assert!(parse("{\"type\":\"message\"\n{\"type\":\"other\"}", None).is_err());
        assert!(parse(r#"{"type":"token_usage_record"}"#, None).is_err());
    }

    #[test]
    fn requires_a_codex_session_reference() {
        assert!(load("w:p1", None).is_err());
        let droid = AgentSession {
            agent: "droid".into(),
            kind: "id".into(),
            source: "herdr:droid".into(),
            value: ID.into(),
        };
        assert!(load("w:p1", Some(&droid)).is_err());
    }

    #[test]
    fn finds_session_file_by_exact_id() {
        let root = env::temp_dir().join(format!("herdr-lens-codex-find-{}", std::process::id()));
        let day = root.join("2026").join("09").join("28");
        fs::create_dir_all(&day).unwrap();
        fs::write(
            day.join(format!("rollout-2026-09-28T16-37-48-{ID}.jsonl")),
            "",
        )
        .unwrap();

        assert_eq!(
            find_by_id(&root, ID).unwrap(),
            day.join(format!("rollout-2026-09-28T16-37-48-{ID}.jsonl"))
        );
        assert!(find_by_id(&root, "abc-123").is_err());
        assert!(find_by_id(&root, "../etc/passwd").is_err());

        fs::remove_dir_all(&root).unwrap();
    }
}
