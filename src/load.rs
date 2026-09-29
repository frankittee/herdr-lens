//! Builds the conversation for a resolved target pane.

use anyhow::Result;

use crate::context::Target;
use crate::conversation::{Conversation, Source, Transcript};
use crate::herdr::Herdr;
use crate::{parser, session};

/// Which source the viewer asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// The session log when Herdr reports one, otherwise terminal scrollback.
    #[default]
    Auto,
    Terminal,
}

impl Choice {
    /// Reads `source=terminal` from a request query string; anything else means `Auto`.
    pub fn from_query(url: &str) -> Self {
        let query = url.split_once('?').map_or("", |(_, query)| query);
        let query = query.split('#').next().unwrap_or("");
        if query.split('&').any(|pair| pair == "source=terminal") {
            Self::Terminal
        } else {
            Self::Auto
        }
    }
}

pub fn conversation(herdr: &Herdr, target: Target, choice: Choice) -> Result<Conversation> {
    let reported = has_session_reader(&target.agent) && target.pane.agent_session.is_some();
    let mut session_error = None;
    if reported && choice == Choice::Auto {
        match read_session(&target) {
            Ok(transcript) => return Ok(build_conversation(target, transcript, true, None)),
            // A session Herdr reports but that cannot be read (wrong id, log not written yet)
            // must not hide the pane; its scrollback is still there.
            Err(err) => {
                eprintln!("session unavailable for {}: {err:#}", target.pane.pane_id);
                session_error = Some(format!("{err:#}"));
            }
        }
    }
    let transcript = read_terminal(herdr, &target)?;
    let session_available = reported && session_error.is_none();
    Ok(build_conversation(
        target,
        transcript,
        session_available,
        session_error,
    ))
}

fn has_session_reader(agent: &str) -> bool {
    matches!(agent, "droid" | "codex")
}

/// Reads only the session Herdr reports for the pane, never another one.
fn read_session(target: &Target) -> Result<Transcript> {
    let pane = &target.pane;
    let session = pane.agent_session.as_ref();
    match target.agent.as_str() {
        "droid" => session::droid::load(&pane.pane_id, session),
        "codex" => session::codex::load(&pane.pane_id, session),
        agent => unreachable!("no session reader for {agent}"),
    }
}

fn read_terminal(herdr: &Herdr, target: &Target) -> Result<Transcript> {
    let ansi = herdr.read_agent_ansi(&target.pane.pane_id)?;
    Ok(Transcript {
        source: Source::Terminal,
        title: None,
        messages: parser::parse_terminal(&target.agent, &ansi),
    })
}

fn build_conversation(
    target: Target,
    transcript: Transcript,
    session_available: bool,
    session_error: Option<String>,
) -> Conversation {
    let pane = target.pane;
    let mut sources = vec![Source::Terminal];
    if session_available {
        sources.insert(0, Source::Session);
    }
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
        sources,
        session_error,
        messages: transcript.messages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_terminal_choice_from_query() {
        assert_eq!(Choice::from_query("/t/api/conversation"), Choice::Auto);
        assert_eq!(
            Choice::from_query("/t/api/conversation?source=terminal"),
            Choice::Terminal
        );
        assert_eq!(
            Choice::from_query("/t/api/conversation?x=1&source=terminal"),
            Choice::Terminal
        );
        assert_eq!(
            Choice::from_query("/t/api/conversation?source=session"),
            Choice::Auto
        );
        assert_eq!(
            Choice::from_query("/t/api/conversation#source=terminal"),
            Choice::Auto
        );
    }
}
