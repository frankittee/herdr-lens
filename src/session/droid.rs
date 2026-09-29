//! Reads Droid's session log, one JSONL file per session under
//! `~/.factory/sessions/<cwd-slug>/<session-id>.jsonl`.
//!
//! Only the session Herdr reports for the target pane is opened, and the log's own
//! `session_start` id must match it.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::conversation::{Message, Role, Source, ToolCall, Transcript};
use crate::herdr::AgentSession;

/// Context Droid injects for the model inside user messages; never shown in the TUI.
const REMINDER_PREFIX: &str = "<system-reminder>";
const REMINDER_SUFFIX: &str = "</system-reminder>";
const HIDDEN_VISIBILITY: &str = "llm_only";
/// Tool input fields that best summarize a call, in priority order.
const TITLE_KEYS: [&str; 6] = ["command", "file_path", "url", "query", "pattern", "path"];

pub fn load(pane_id: &str, session: Option<&AgentSession>) -> Result<Transcript> {
    let session = session.with_context(|| {
        format!("Herdr has not reported a Droid session for pane {pane_id} yet")
    })?;
    ensure!(
        session.agent == "droid",
        "pane {pane_id} reports a {} session, not a Droid session",
        session.agent
    );
    let (path, expected_id) = match session.kind.as_str() {
        "id" => (
            find_by_id(&sessions_root()?, &session.value)?,
            Some(session.value.as_str()),
        ),
        "path" => (PathBuf::from(&session.value), None),
        other => bail!("unsupported Droid session reference kind {other:?}"),
    };
    let jsonl = fs::read_to_string(&path)
        .with_context(|| format!("failed to read Droid session log {}", path.display()))?;
    parse(&jsonl, expected_id)
}

fn sessions_root() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".factory").join("sessions"))
}

/// Session ids are unique, so the file is looked up in every working-directory group.
fn find_by_id(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(is_session_id(id), "invalid Droid session id {id:?}");
    let file_name = format!("{id}.jsonl");
    let direct = root.join(&file_name);
    if direct.is_file() {
        return Ok(direct);
    }
    let groups =
        fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))?;
    for group in groups.flatten() {
        let candidate = group.path().join(&file_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!(
        "Droid session log {file_name} not found under {}",
        root.display()
    )
}

/// Also rejects path separators, so the id cannot escape the sessions directory.
fn is_session_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry {
    SessionStart {
        id: String,
        title: Option<String>,
    },
    Message {
        timestamp: Option<String>,
        message: Body,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Body {
    role: String,
    #[serde(default)]
    content: Vec<Block>,
    visibility: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

pub fn parse(jsonl: &str, expected_id: Option<&str>) -> Result<Transcript> {
    let lines: Vec<(usize, &str)> = jsonl
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .collect();
    let mut builder = Builder::default();
    let mut title = None;
    let mut started = false;

    for (i, &(number, line)) in lines.iter().enumerate() {
        let entry = match serde_json::from_str::<Entry>(line) {
            Ok(entry) => entry,
            // Droid appends while it works, so the last line may be partially written.
            Err(_) if i + 1 == lines.len() => break,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("malformed Droid session log line {}", number + 1))
            }
        };
        match entry {
            Entry::SessionStart { id, title: t } => {
                if let Some(expected) = expected_id {
                    ensure!(
                        id == expected,
                        "Droid session log belongs to {id}, not {expected}"
                    );
                }
                title = t;
                started = true;
            }
            Entry::Message { timestamp, message } => builder.add(message, timestamp),
            Entry::Other => {}
        }
    }

    ensure!(started, "not a Droid session log: no session_start entry");
    Ok(Transcript {
        source: Source::Session,
        title,
        messages: builder.messages,
    })
}

#[derive(Default)]
struct Builder {
    messages: Vec<Message>,
    /// Tool call id -> index of its message, so results can be attached to their call.
    tool_calls: HashMap<String, usize>,
}

impl Builder {
    fn add(&mut self, body: Body, timestamp: Option<String>) {
        if body.visibility.as_deref() == Some(HIDDEN_VISIBILITY) {
            return;
        }
        let text_role = match body.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => Role::System,
        };
        for block in body.content {
            match block {
                Block::Text { text } if is_visible_text(&text) => {
                    self.push(Message::new(text_role, text.trim()), &timestamp);
                }
                Block::Thinking { thinking } if !thinking.trim().is_empty() => {
                    self.push(Message::new(Role::Reasoning, thinking.trim()), &timestamp);
                }
                Block::ToolUse { id, name, input } => {
                    let mut message = Message::tool(tool_title(&name, &input));
                    message.tool = Some(ToolCall {
                        name,
                        input,
                        pending: true,
                        is_error: false,
                    });
                    self.tool_calls.insert(id, self.messages.len());
                    self.push(message, &timestamp);
                }
                Block::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => self.attach_result(&tool_use_id, &content, is_error, &timestamp),
                _ => {}
            }
        }
    }

    fn push(&mut self, mut message: Message, timestamp: &Option<String>) {
        message.timestamp = timestamp.clone();
        self.messages.push(message);
    }

    fn attach_result(&mut self, id: &str, content: &Value, is_error: bool, ts: &Option<String>) {
        let text = strip_trailing_reminder(&result_text(content)).to_owned();
        let Some(&index) = self.tool_calls.get(id) else {
            // Keep results whose call is missing from the log rather than dropping output.
            self.push(Message::new(Role::Tool, text), ts);
            return;
        };
        let message = &mut self.messages[index];
        message.text = text;
        if let Some(tool) = message.tool.as_mut() {
            tool.pending = false;
            tool.is_error = is_error;
        }
    }
}

fn is_visible_text(text: &str) -> bool {
    let text = text.trim_start();
    !text.is_empty() && !text.starts_with(REMINDER_PREFIX)
}

/// Droid appends notes for the model to some tool results, e.g. `Read` adds
/// `<system-reminder>[Showing lines 1-75 of 134 total lines]</system-reminder>`.
/// Only a block that ends the result is removed, so command output quoting the tag stays intact.
fn strip_trailing_reminder(text: &str) -> &str {
    let trimmed = text.trim_end();
    match trimmed.rfind(REMINDER_PREFIX) {
        Some(start) if trimmed.ends_with(REMINDER_SUFFIX) => trimmed[..start].trim_end(),
        _ => text,
    }
}

fn tool_title(name: &str, input: &Value) -> String {
    let detail = TITLE_KEYS
        .iter()
        .find_map(|key| input.get(key).and_then(Value::as_str));
    match detail {
        Some(detail) => format!("{name} {detail}"),
        None => name.to_owned(),
    }
}

/// Tool results are a string, or a list of content parts of which only text is shown.
fn result_text(content: &Value) -> String {
    match content {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|part| match part.get("text").and_then(Value::as_str) {
                Some(text) => text.to_owned(),
                None => format!(
                    "[{}]",
                    part.get("type").and_then(Value::as_str).unwrap_or("?")
                ),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "8c08e3ff-84a0-4706-bebe-018af371a9f3";

    fn sample() -> String {
        [
            &format!(r#"{{"type":"session_start","id":"{ID}","title":"Fix parser","cwd":"/repo"}}"#),
            r#"{"type":"message","timestamp":"t0","message":{"role":"user","visibility":"llm_only","content":[{"type":"text","text":"hidden context"}]}}"#,
            r#"{"type":"message","timestamp":"t1","message":{"role":"user","content":[{"type":"text","text":"<system-reminder>env</system-reminder>"},{"type":"text","text":"Fix the parser\n"}]}}"#,
            r#"{"type":"message","timestamp":"t2","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Look first.","signature":"x"},{"type":"text","text":"Checking."},{"type":"tool_use","id":"c1","name":"Execute","input":{"command":"cargo test","riskLevel":"low"}},{"type":"tool_use","id":"c2","name":"Read","input":{"file_path":"/repo/a.rs"}}]}}"#,
            r#"{"type":"todo_state","todos":"1. [pending] x"}"#,
            r#"{"type":"message","timestamp":"t3","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"ok","is_error":false},{"type":"tool_result","tool_use_id":"c2","content":[{"type":"text","text":"fn a()"},{"type":"image"}],"is_error":true}]}}"#,
            r#"{"type":"message","timestamp":"t4","message":{"role":"assistant","content":[{"type":"tool_use","id":"c3","name":"TodoWrite","input":{"todos":"1. x"}}]}}"#,
            r#"{"type":"message","timestamp":"t5","message":{"role":"assis"#,
        ]
        .join("\n")
    }

    #[test]
    fn parses_visible_history_and_pairs_tool_results() {
        let transcript = parse(&sample(), Some(ID)).unwrap();
        assert_eq!(transcript.source, Source::Session);
        assert_eq!(transcript.title.as_deref(), Some("Fix parser"));

        let m = &transcript.messages;
        let roles: Vec<Role> = m.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [
                Role::User,
                Role::Reasoning,
                Role::Assistant,
                Role::Tool,
                Role::Tool,
                Role::Tool
            ]
        );
        assert_eq!(m[0].text, "Fix the parser");
        assert_eq!(m[0].timestamp.as_deref(), Some("t1"));
        assert_eq!(m[1].text, "Look first.");

        assert_eq!(m[3].title.as_deref(), Some("Execute cargo test"));
        assert_eq!(m[3].text, "ok");
        let exec = m[3].tool.as_ref().unwrap();
        assert_eq!(exec.name, "Execute");
        assert_eq!(exec.input["riskLevel"], "low");
        assert!(!exec.pending && !exec.is_error);

        assert_eq!(m[4].title.as_deref(), Some("Read /repo/a.rs"));
        assert_eq!(m[4].text, "fn a()\n[image]");
        assert!(m[4].tool.as_ref().unwrap().is_error);

        assert_eq!(m[5].title.as_deref(), Some("TodoWrite"));
        assert!(m[5].tool.as_ref().unwrap().pending);
    }

    /// A real Droid session log. Its id and message mix depend on the local fixture.
    #[test]
    fn parses_real_droid_session_log() {
        let jsonl = include_str!("../../fixtures/droid/session.jsonl");
        let transcript = parse(jsonl, None).unwrap();
        assert_eq!(transcript.source, Source::Session);
        assert!(!transcript.messages.is_empty());
    }

    #[test]
    fn strips_only_trailing_reminders_from_results() {
        assert_eq!(
            strip_trailing_reminder(
                "1\tfn a()\n\n<system-reminder>[Showing lines 1-1]</system-reminder>\n"
            ),
            "1\tfn a()"
        );
        let quoted = "grep output: <system-reminder>\nmore";
        assert_eq!(strip_trailing_reminder(quoted), quoted);
    }

    #[test]
    fn rejects_log_of_another_session() {
        let err = parse(&sample(), Some("0000")).unwrap_err();
        assert!(err.to_string().contains("belongs to"));
    }

    #[test]
    fn rejects_malformed_middle_line_and_missing_start() {
        assert!(parse("{\"type\":\"message\"\n{\"type\":\"other\"}", None).is_err());
        assert!(parse(r#"{"type":"todo_state"}"#, None).is_err());
    }

    #[test]
    fn requires_a_droid_session_reference() {
        assert!(load("w:p1", None).is_err());
        let claude = AgentSession {
            agent: "claude".into(),
            kind: "id".into(),
            source: "herdr:claude".into(),
            value: ID.into(),
        };
        assert!(load("w:p1", Some(&claude)).is_err());
    }

    #[test]
    fn finds_session_file_by_exact_id() {
        let root = env::temp_dir().join(format!("herdr-lens-droid-{}", std::process::id()));
        let group = root.join("-repo");
        fs::create_dir_all(&group).unwrap();
        fs::write(group.join(format!("{ID}.jsonl")), "").unwrap();

        assert_eq!(
            find_by_id(&root, ID).unwrap(),
            group.join(format!("{ID}.jsonl"))
        );
        assert!(find_by_id(&root, "abc-123").is_err());
        assert!(find_by_id(&root, "../etc/passwd").is_err());

        fs::remove_dir_all(&root).unwrap();
    }
}
