mod agents;
mod context;
mod conversation;
mod herdr;
mod launch;
mod load;
mod parser;
mod server;
mod session;
mod terminal;

use std::process::ExitCode;

use anyhow::{Context, Result};
use usage::{Args, Cli, Subcommands};

use context::PluginContext;
use conversation::Conversation;
use herdr::Herdr;
use server::{Pin, Viewer};

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
    Serve(Serve),
}

/// Plugin action: open the invoking pane's conversation in the browser
#[derive(Args)]
struct Open {}

/// Print the parsed conversation as JSON for debugging
#[derive(Args)]
struct Dump {}

/// Serve the viewer for one pane on loopback; started by `open`
#[derive(Args)]
#[usage(hide)]
struct Serve {
    /// Pane whose conversation is served
    #[usage(long)]
    pane: String,
}

fn main() -> ExitCode {
    let cli = HerdrLens::parse();
    let herdr = Herdr::from_env();
    let (result, notify_on_error) = match cli.command {
        Command::Open(_) => (open(&herdr), true),
        Command::Dump(_) => (dump(&herdr), false),
        Command::Serve(args) => (serve(&herdr, &args.pane), false),
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

/// Reads the conversation once so problems surface as a Herdr toast, then hands off to the server.
fn open(herdr: &Herdr) -> Result<()> {
    let conversation = load_invoking_conversation(herdr)?;
    let url = launch::start_server(&conversation.pane_id)?;
    launch::open_browser(&url)
}

fn serve(herdr: &Herdr, pane_id: &str) -> Result<()> {
    let viewer = bind_viewer(herdr, pane_id);
    match &viewer {
        Ok(viewer) => launch::report_ready(&viewer.url()),
        Err(err) => launch::report_failure(err),
    }
    viewer?.run(herdr)
}

fn bind_viewer(herdr: &Herdr, pane_id: &str) -> Result<Viewer> {
    let target = context::resolve_pane(herdr, pane_id)?;
    let pin = Pin {
        pane_id: target.pane.pane_id,
        terminal_id: target.pane.terminal_id,
        agent: target.agent,
    };
    Viewer::bind(launch::dist_dir()?, pin)
}

fn dump(herdr: &Herdr) -> Result<()> {
    let conversation = load_invoking_conversation(herdr)?;
    let json =
        serde_json::to_string_pretty(&conversation).context("failed to encode conversation")?;
    println!("{json}");
    Ok(())
}

fn load_invoking_conversation(herdr: &Herdr) -> Result<Conversation> {
    let context = PluginContext::from_env()?;
    let target = context::resolve(herdr, &context)?;
    load::conversation(herdr, target)
}
