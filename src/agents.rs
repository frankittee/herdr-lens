//! Agent overview for the sidebar: pane metadata only, never another pane's conversation.

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::herdr::{AgentInfo, Herdr, WorkspaceInfo};

#[derive(Debug, Serialize)]
pub struct AgentList {
    pub agents: Vec<AgentSummary>,
}

/// Mirrors `AgentSummary` in web/src/api.ts.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct AgentSummary {
    pub pane_id: String,
    pub workspace_id: String,
    pub workspace_label: Option<String>,
    pub tab_id: String,
    pub agent: String,
    pub agent_status: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub focused: bool,
}

pub fn list(herdr: &Herdr) -> Result<AgentList> {
    Ok(summarize(herdr.agents()?, &herdr.workspaces()?))
}

fn summarize(agents: Vec<AgentInfo>, workspaces: &[WorkspaceInfo]) -> AgentList {
    let labels: HashMap<&str, &str> = workspaces
        .iter()
        .filter_map(|ws| Some((ws.workspace_id.as_str(), ws.label.as_deref()?)))
        .collect();
    let agents = agents
        .into_iter()
        .map(|agent| AgentSummary {
            workspace_label: labels
                .get(agent.workspace_id.as_str())
                .map(|l| l.to_string()),
            title: agent
                .terminal_title_stripped
                .as_deref()
                .and_then(clean_title),
            pane_id: agent.pane_id,
            workspace_id: agent.workspace_id,
            tab_id: agent.tab_id,
            agent: agent.agent,
            agent_status: agent.agent_status,
            cwd: agent.cwd,
            focused: agent.focused,
        })
        .collect();
    AgentList { agents }
}

/// Agents prefix terminal titles with status glyphs (Droid uses `⛬`) that Herdr keeps.
fn clean_title(title: &str) -> Option<String> {
    let title =
        title.trim_start_matches(|c: char| !c.is_alphanumeric() && !c.is_ascii_punctuation());
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(pane: &str, workspace: &str, title: Option<&str>) -> AgentInfo {
        AgentInfo {
            pane_id: pane.into(),
            workspace_id: workspace.into(),
            tab_id: format!("{workspace}:t1"),
            agent: "droid".into(),
            agent_status: "idle".into(),
            terminal_title_stripped: title.map(str::to_owned),
            cwd: None,
            focused: false,
        }
    }

    #[test]
    fn joins_workspace_labels_and_cleans_titles() {
        let workspaces = [WorkspaceInfo {
            workspace_id: "w1".into(),
            label: Some("lens".into()),
        }];
        let list = summarize(
            vec![
                agent("w1:p1", "w1", Some("⛬ Build the UI")),
                agent("w2:p1", "w2", Some("⛬ ")),
            ],
            &workspaces,
        );
        assert_eq!(list.agents[0].workspace_label.as_deref(), Some("lens"));
        assert_eq!(list.agents[0].title.as_deref(), Some("Build the UI"));
        assert_eq!(list.agents[1].workspace_label, None);
        assert_eq!(list.agents[1].title, None);
    }

    #[test]
    fn keeps_titles_that_start_with_text_or_punctuation() {
        assert_eq!(clean_title("修复登录").as_deref(), Some("修复登录"));
        assert_eq!(clean_title("(wip) tests").as_deref(), Some("(wip) tests"));
    }
}
