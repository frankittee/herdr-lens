//! Builds the conversation for a resolved target pane.

use anyhow::Result;

use crate::context::Target;
use crate::conversation::{Conversation, Source, Transcript};
use crate::herdr::Herdr;
use crate::{parser, session};

pub fn conversation(herdr: &Herdr, target: Target) -> Result<Conversation> {
    let transcript = read_transcript(herdr, &target)?;
    Ok(build_conversation(target, transcript))
}

/// Agents with a session reader get their complete history from the session Herdr reports for
/// the pane. Other agents fall back to terminal scrollback, marked as `source: terminal`.
fn read_transcript(herdr: &Herdr, target: &Target) -> Result<Transcript> {
    let pane = &target.pane;
    match target.agent.as_str() {
        "droid" => session::droid::load(&pane.pane_id, pane.agent_session.as_ref()),
        "codex" => session::codex::load(&pane.pane_id, pane.agent_session.as_ref()),
        agent => {
            let ansi = herdr.read_agent_ansi(&pane.pane_id)?;
            Ok(Transcript {
                source: Source::Terminal,
                title: None,
                messages: parser::parse_terminal(agent, &ansi),
            })
        }
    }
}

fn build_conversation(target: Target, transcript: Transcript) -> Conversation {
    let pane = target.pane;
    Conversation {
        agent: target.agent,
        pane_id: pane.pane_id,
        workspace_id: pane.workspace_id,
        tab_id: pane.tab_id,
        cwd: pane.cwd,
        status: pane.agent_status,
        session: pane.agent_session,
        title: transcript.title,
        source: transcript.source,
        messages: transcript.messages,
    }
}
