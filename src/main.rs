mod context;
mod conversation;
mod herdr;
mod parser;
mod session;
mod terminal;

use std::process::ExitCode;

use anyhow::{Context, Result};
use usage::{Args, Cli, Subcommands};

use context::{PluginContext, Target};
use conversation::{Conversation, Source, Transcript};
use herdr::Herdr;

/// Open the conversation of the agent in the invoking Herdr pane
#[derive(Cli)]
#[usage(bin = "herdr-lens", version = "0.1.0")]
struct HerdrLens {
    #[usage(subcommand)]
    command: Command,
}

#[derive(Subcommands)]
enum Command {
    Open(Open),
    Dump(Dump),
}

/// Plugin action: read the invoking pane's conversation
#[derive(Args)]
struct Open {}

/// Print the parsed conversation as JSON for debugging
#[derive(Args)]
struct Dump {}

fn main() -> ExitCode {
    let cli = HerdrLens::parse();
    let herdr = Herdr::from_env();
    let (result, notify_on_error) = match cli.command {
        Command::Open(_) => (open(&herdr), true),
        Command::Dump(_) => (dump(&herdr), false),
    };
    let Err(err) = result else {
        return ExitCode::SUCCESS;
    };
    let message = format!("{err:#}");
    eprintln!("herdr-lens: {message}");
    if notify_on_error {
        herdr.notify("Herdr Lens", &message);
    }
    ExitCode::FAILURE
}

/// The web viewer is not implemented yet, so this only reports a summary.
fn open(herdr: &Herdr) -> Result<()> {
    let conversation = load_conversation(herdr)?;
    eprintln!(
        "Herdr Lens read {} messages from {} in pane {}; the web viewer is not implemented yet.",
        conversation.messages.len(),
        conversation.agent,
        conversation.pane_id
    );
    Ok(())
}

fn dump(herdr: &Herdr) -> Result<()> {
    let conversation = load_conversation(herdr)?;
    let json =
        serde_json::to_string_pretty(&conversation).context("failed to encode conversation")?;
    println!("{json}");
    Ok(())
}

fn load_conversation(herdr: &Herdr) -> Result<Conversation> {
    let context = PluginContext::from_env()?;
    let target = context::resolve(herdr, &context)?;
    let transcript = read_transcript(herdr, &target)?;
    Ok(build_conversation(target, transcript))
}

/// Agents with a session reader get their complete history from the session Herdr reports for
/// the pane. Other agents fall back to terminal scrollback, marked as `source: terminal`.
fn read_transcript(herdr: &Herdr, target: &Target) -> Result<Transcript> {
    let pane = &target.pane;
    match target.agent.as_str() {
        "droid" => session::droid::load(&pane.pane_id, pane.agent_session.as_ref()),
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
