//! Thin wrapper around the Herdr CLI, which is the plugin API.

use std::env;
use std::process::{Command, Output};

use anyhow::{anyhow, Context, Result};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};

/// Upper bound on scrollback lines requested from Herdr; Herdr returns fewer when it has less.
const READ_LINES: &str = "100000";

/// Pane fields Herdr Lens needs from `herdr pane get`.
#[derive(Debug, Clone, Deserialize)]
pub struct PaneInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: Option<String>,
    #[serde(default)]
    pub agent_status: String,
    pub agent_session: Option<AgentSession>,
    pub cwd: Option<String>,
}

/// Session identity reported by the agent integration. `kind` is `id` or `path`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentSession {
    pub agent: String,
    pub kind: String,
    pub source: String,
    pub value: String,
}

/// Herdr CLI JSON reply: `{"id": .., "result": ..}` or `{"id": .., "error": ..}`.
#[derive(Deserialize)]
struct Envelope<T> {
    result: Option<T>,
    error: Option<ErrorBody>,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct PaneResult {
    pane: PaneInfo,
}

pub struct Herdr {
    bin: String,
}

impl Herdr {
    pub fn from_env() -> Self {
        let bin = env::var("HERDR_BIN_PATH")
            .ok()
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| "herdr".to_owned());
        Self { bin }
    }

    pub fn pane(&self, pane_id: &str) -> Result<PaneInfo> {
        let result: PaneResult = self.json(&["pane", "get", pane_id])?;
        Ok(result.pane)
    }

    /// Reads the agent's terminal scrollback with ANSI styling, which the parsers use as role hints.
    pub fn read_agent_ansi(&self, pane_id: &str) -> Result<String> {
        let args = [
            "agent",
            "read",
            pane_id,
            "--source",
            "recent-unwrapped",
            "--lines",
            READ_LINES,
            "--format",
            "ansi",
        ];
        let output = self.run_ok(&args)?;
        String::from_utf8(output.stdout).context("herdr agent read returned non-UTF-8 output")
    }

    /// Shows a Herdr toast. Best effort: failures are ignored because this is only used for reporting.
    pub fn notify(&self, title: &str, body: &str) {
        let _ = self.run(&["notification", "show", title, "--body", body]);
    }

    fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        let output = self.run_ok(args)?;
        parse_envelope(&output.stdout)
    }

    fn run_ok(&self, args: &[&str]) -> Result<Output> {
        let output = self.run(args)?;
        if !output.status.success() {
            return Err(command_error(args, &output));
        }
        Ok(output)
    }

    fn run(&self, args: &[&str]) -> Result<Output> {
        Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {}", self.bin))
    }
}

/// Returns `result` from a Herdr CLI JSON envelope, or turns `error` into an error.
fn parse_envelope<T: DeserializeOwned>(stdout: &[u8]) -> Result<T> {
    let envelope: Envelope<T> =
        serde_json::from_slice(stdout).context("unexpected herdr JSON output")?;
    if let Some(error) = envelope.error {
        return Err(envelope_error(error));
    }
    envelope
        .result
        .ok_or_else(|| anyhow!("herdr JSON output has no result"))
}

fn envelope_error(error: ErrorBody) -> anyhow::Error {
    anyhow!("herdr {}: {}", error.code, error.message)
}

fn command_error(args: &[&str], output: &Output) -> anyhow::Error {
    // Herdr reports failures as a JSON error envelope on stderr, even for text commands.
    for stream in [&output.stderr, &output.stdout] {
        if let Ok(Envelope::<IgnoredAny> {
            error: Some(error), ..
        }) = serde_json::from_slice(stream)
        {
            return envelope_error(error);
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let command = args.first().copied().unwrap_or("");
    match stderr.lines().next().map(str::trim) {
        Some(detail) if !detail.is_empty() => {
            anyhow!("herdr {command} failed ({}): {detail}", output.status)
        }
        _ => anyhow!("herdr {command} failed ({})", output.status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_returns_result() {
        let result: PaneResult = parse_envelope(
            br#"{"id":"x","result":{"pane":{"pane_id":"w:p1","workspace_id":"w","tab_id":"w:t1"}}}"#,
        )
        .unwrap();
        assert_eq!(result.pane.pane_id, "w:p1");
        assert!(result.pane.agent.is_none());
    }

    #[test]
    fn envelope_reports_error() {
        let err = parse_envelope::<PaneResult>(
            br#"{"error":{"code":"pane_not_found","message":"pane w:p9 not found"},"id":"x"}"#,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "herdr pane_not_found: pane w:p9 not found");
    }

    #[test]
    fn envelope_rejects_missing_result() {
        assert!(parse_envelope::<PaneResult>(br#"{"id":"x"}"#).is_err());
    }

    #[test]
    fn pane_info_accepts_missing_agent() {
        let pane: PaneInfo = serde_json::from_str(
            r#"{"pane_id":"w:p1","workspace_id":"w","tab_id":"w:t1","terminal_id":"t","focused":true,"agent_status":"unknown","revision":1}"#,
        )
        .unwrap();
        assert!(pane.agent.is_none());
        assert!(pane.agent_session.is_none());
    }
}
