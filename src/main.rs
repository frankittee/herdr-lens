mod agents;
mod context;
mod conversation;
mod herdr;
mod launch;
mod load;
mod parser;
mod server;
mod session;
mod shared;
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
    Startup(Startup),
    Open(Open),
    Link(Link),
    Dump(Dump),
    Serve(Serve),
}

/// Start the shared viewer when the Herdr server starts
#[derive(Args)]
struct Startup {}

/// Plugin action: open the invoking pane's conversation in the browser
#[derive(Args)]
struct Open {}

/// Plugin action: show a viewer URL for manual SSH forwarding
#[derive(Args)]
struct Link {}

/// Print the parsed conversation as JSON for debugging
#[derive(Args)]
struct Dump {}

/// Serve one pane or the shared startup viewer on loopback
#[derive(Args)]
#[usage(hide)]
struct Serve {
    /// Pane whose conversation is served
    #[usage(long)]
    pane: Option<String>,
    /// Serve pane URLs from the startup viewer
    #[usage(long)]
    shared: bool,
}

fn main() -> ExitCode {
    let cli = HerdrLens::parse();
    let herdr = Herdr::from_env();
    let (result, notify_on_error) = match cli.command {
        Command::Startup(_) => (launch::start_shared(), false),
        Command::Open(_) => (open(&herdr), true),
        Command::Link(_) => (link(&herdr), true),
        Command::Dump(_) => (dump(&herdr), false),
        Command::Serve(args) => (serve(&herdr, args.pane.as_deref(), args.shared), false),
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
    let pin = invoking_pin(herdr)?;
    let address = shared::read()?;
    let url = server::shared_url(address.port, &address.token, &pin)?;
    launch::open_browser(&url)
}

/// Starts the remote viewer and delivers its URL through the Herdr client notification.
fn link(herdr: &Herdr) -> Result<()> {
    let pin = invoking_pin(herdr)?;
    let address = shared::read()?;
    let url = server::shared_url(address.port, &address.token, &pin)?;
    herdr.show_notification("Herdr Lens: viewer link", &url)
}

fn serve(herdr: &Herdr, pane_id: Option<&str>, shared: bool) -> Result<()> {
    let viewer = if shared && pane_id.is_none() {
        Viewer::bind_shared(launch::dist_dir()?)
    } else if shared {
        anyhow::bail!("serve accepts either --pane or --shared");
    } else {
        bind_viewer(herdr, pane_id.context("serve requires --pane or --shared")?)
    };
    let viewer = match viewer {
        Ok(viewer) => viewer,
        Err(err) => {
            launch::report_failure(&err);
            return Err(err);
        }
    };
    if shared {
        let (port, token) = viewer.address();
        if let Err(err) = shared::publish(port, token) {
            launch::report_failure(&err);
            return Err(err);
        }
        launch::report_ready("ok");
    } else {
        launch::report_ready(&viewer.url());
    }
    viewer.run(herdr)
}

fn invoking_pin(herdr: &Herdr) -> Result<Pin> {
    let context = PluginContext::from_env()?;
    let target = context::resolve(herdr, &context)?;
    load::conversation(herdr, target.clone())?;
    Ok(Pin {
        pane_id: target.pane.pane_id,
        terminal_id: target.pane.terminal_id,
        agent: target.agent,
    })
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
