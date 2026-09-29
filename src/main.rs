mod agents;
mod context;
mod conversation;
mod copy;
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
    Copy(Copy),
    Dump(Dump),
    Serve(Serve),
    CopyPane(CopyPane),
}

/// Popup pane: copy the viewer URL through the terminal and show it
#[derive(Args)]
#[usage(hide)]
struct CopyPane {}

/// Start the shared viewer when the Herdr server starts
#[derive(Args)]
struct Startup {}

/// Plugin action: open the invoking pane's conversation in the browser
#[derive(Args)]
struct Open {}

/// Plugin action: copy and show a viewer URL for manual SSH forwarding
#[derive(Args)]
struct Copy {}

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
        Command::Copy(_) => (copy(&herdr), true),
        Command::Dump(_) => (dump(&herdr), false),
        Command::Serve(args) => (serve(&herdr, args.pane.as_deref(), args.shared), false),
        Command::CopyPane(_) => (copy_pane(&herdr), false),
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
    let url = invoking_url(herdr)?;
    launch::open_browser(&url)
}

/// Validates the invoking pane here so problems surface as a toast, then opens the copy popup.
fn copy(herdr: &Herdr) -> Result<()> {
    let pin = invoking_pin(herdr)?;
    copy::open_popup(herdr, &pin.pane_id)
}

/// Runs inside the popup, whose terminal output Herdr forwards to the attached client.
fn copy_pane(herdr: &Herdr) -> Result<()> {
    let result = copy::target_pane()
        .and_then(|pane_id| pin_url(pin_for_pane(herdr, &pane_id)?))
        .and_then(|url| copy::show(&url));
    if let Err(err) = &result {
        copy::show_error(err);
    }
    result
}

fn invoking_url(herdr: &Herdr) -> Result<String> {
    pin_url(invoking_pin(herdr)?)
}

fn pin_url(pin: Pin) -> Result<String> {
    let address = shared_address()?;
    server::shared_url(address.port, &address.token, &pin)
}

/// The startup hook only runs when the Herdr server starts, so start the viewer if it is missing.
fn shared_address() -> Result<shared::Address> {
    if let Ok(address) = shared::read() {
        return Ok(address);
    }
    launch::start_shared()?;
    shared::read()
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
    Viewer::bind(launch::dist_dir()?, pin_for_pane(herdr, pane_id)?)
}

fn pin_for_pane(herdr: &Herdr, pane_id: &str) -> Result<Pin> {
    let target = context::resolve_pane(herdr, pane_id)?;
    Ok(Pin {
        pane_id: target.pane.pane_id,
        terminal_id: target.pane.terminal_id,
        agent: target.agent,
    })
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
