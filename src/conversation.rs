//! Agent-neutral conversation model shared by every parser and served to the browser UI as JSON.

use serde::Serialize;
use serde_json::Value;

use crate::herdr::AgentSession;

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    /// Herdr agent label, e.g. `droid`, `claude`, `codex`.
    pub agent: String,
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub cwd: Option<String>,
    /// Herdr agent status at read time: `idle`, `working`, `blocked`, `done`, or `unknown`.
    pub status: String,
    pub session: Option<AgentSession>,
    /// Session title recorded by the agent, when the source has one.
    pub title: Option<String>,
    pub source: Source,
    pub messages: Vec<Message>,
}

/// Messages from one source, before pane metadata is attached.
#[derive(Debug, Clone)]
pub struct Transcript {
    pub source: Source,
    pub title: Option<String>,
    pub messages: Vec<Message>,
}

/// Where the messages came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The agent's own session log: the complete history.
    Session,
    /// Parsed from terminal scrollback. May miss content that scrolled out of Herdr's buffer.
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    /// Visible thinking or reasoning output.
    Reasoning,
    /// A tool call; `title` holds the call, `text` its output.
    Tool,
    /// Notices from the agent UI, e.g. model switches.
    System,
    /// Raw terminal text that no parser could attribute to a role.
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
    /// RFC 3339 time the agent recorded the message, when the source has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Structured call details for `tool` messages from session sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolCall {
    pub name: String,
    /// Tool arguments exactly as the agent sent them.
    pub input: Value,
    /// No result has been recorded yet, e.g. the tool is still running.
    pub pending: bool,
    pub is_error: bool,
}

impl Message {
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            title: None,
            text: text.into(),
            timestamp: None,
            tool: None,
        }
    }

    pub fn tool(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::new(Role::Tool, "")
        }
    }

    /// Appends one line, skipping leading blank lines.
    pub fn push_line(&mut self, line: &str) {
        push_line(&mut self.text, line);
    }

    /// Removes trailing blank lines left by paragraph separators.
    pub fn finish(&mut self) {
        let trimmed = self.text.trim_end().len();
        self.text.truncate(trimmed);
    }
}

pub fn push_line(buffer: &mut String, line: &str) {
    if buffer.is_empty() {
        if !line.trim().is_empty() {
            buffer.push_str(line);
        }
        return;
    }
    buffer.push('\n');
    buffer.push_str(line);
}
