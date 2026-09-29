//! Resolves the pane and agent that invoked the plugin action.

use std::env;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::herdr::{Herdr, PaneInfo};

/// Subset of Herdr's `PluginInvocationContext` passed in `HERDR_PLUGIN_CONTEXT_JSON`.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct PluginContext {
    pub focused_pane_id: Option<String>,
}

/// The invoking pane together with the agent detected in it.
#[derive(Debug, Clone)]
pub struct Target {
    pub pane: PaneInfo,
    pub agent: String,
}

impl PluginContext {
    /// Reads the context from the environment. A missing variable yields an empty context.
    pub fn from_env() -> Result<Self> {
        match env::var("HERDR_PLUGIN_CONTEXT_JSON") {
            Ok(json) if !json.trim().is_empty() => Self::parse(&json),
            _ => Ok(Self::default()),
        }
    }

    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("invalid HERDR_PLUGIN_CONTEXT_JSON")
    }
}

/// Resolves the invoking pane and requires it to run a detected agent.
pub fn resolve(herdr: &Herdr, context: &PluginContext) -> Result<Target> {
    let env_pane = env::var("HERDR_PANE_ID").ok();
    let pane_id = target_pane_id(context, env_pane.as_deref())?;
    resolve_pane(herdr, &pane_id)
}

/// Reads a known pane and requires it to run a detected agent.
pub fn resolve_pane(herdr: &Herdr, pane_id: &str) -> Result<Target> {
    let pane = herdr.pane(pane_id)?;
    let agent = require_agent(&pane)?;
    Ok(Target { pane, agent })
}

/// Picks the pane from the invocation context, then from `HERDR_PANE_ID`.
/// There is deliberately no fallback to whichever pane is focused now.
pub fn target_pane_id(context: &PluginContext, env_pane: Option<&str>) -> Result<String> {
    non_empty(context.focused_pane_id.as_deref())
        .or_else(|| non_empty(env_pane))
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "no target pane: run Herdr Lens as a pane action so Herdr passes the invoking pane"
            )
        })
}

pub fn require_agent(pane: &PaneInfo) -> Result<String> {
    non_empty(pane.agent.as_deref())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "pane {} has no detected agent; open Herdr Lens from a pane running a coding agent",
                pane.pane_id
            )
        })
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(agent: Option<&str>) -> PaneInfo {
        PaneInfo {
            pane_id: "w:p1".into(),
            workspace_id: "w".into(),
            tab_id: "w:t1".into(),
            agent: agent.map(str::to_owned),
            agent_status: "idle".into(),
            agent_session: None,
            cwd: None,
            terminal_id: None,
        }
    }

    #[test]
    fn parses_context_and_ignores_unknown_fields() {
        let context = PluginContext::parse(
            r#"{"focused_pane_id":"w:p2","focused_pane_agent":"droid","workspace_id":"w","selected_text":null,"extra":1}"#,
        )
        .unwrap();
        assert_eq!(context.focused_pane_id.as_deref(), Some("w:p2"));
    }

    #[test]
    fn rejects_invalid_context_json() {
        assert!(PluginContext::parse("not json").is_err());
    }

    #[test]
    fn prefers_context_pane_over_env() {
        let context = PluginContext {
            focused_pane_id: Some("w:p2".into()),
        };
        assert_eq!(target_pane_id(&context, Some("w:p1")).unwrap(), "w:p2");
    }

    #[test]
    fn falls_back_to_env_pane() {
        let context = PluginContext {
            focused_pane_id: Some(" ".into()),
        };
        assert_eq!(target_pane_id(&context, Some("w:p1")).unwrap(), "w:p1");
    }

    #[test]
    fn errors_without_any_pane() {
        assert!(target_pane_id(&PluginContext::default(), None).is_err());
    }

    #[test]
    fn requires_detected_agent() {
        assert_eq!(require_agent(&pane(Some("droid"))).unwrap(), "droid");
        let err = require_agent(&pane(None)).unwrap_err();
        assert!(err.to_string().contains("pane w:p1 has no detected agent"));
        assert!(require_agent(&pane(Some(""))).is_err());
    }
}
