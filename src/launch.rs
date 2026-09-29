//! Starts the viewer server in the background and opens it in the browser.
//!
//! Plugin actions should return promptly, so `open` re-executes this binary as `serve` in its own
//! process group and waits only for the first stdout line: `ready <url>` or `error <message>`.

use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

const LOG_FILE: &str = "server.log";

pub fn start_server(pane_id: &str) -> Result<String> {
    let exe = env::current_exe().context("failed to locate the herdr-lens binary")?;
    let mut child = Command::new(exe)
        .args(["serve", "--pane", pane_id])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log_target())
        .process_group(0)
        .spawn()
        .context("failed to start the viewer server")?;
    let stdout = child.stdout.take().context("viewer server has no stdout")?;
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .context("failed to read from the viewer server")?;
    let line = line.trim_end();
    if let Some(url) = line.strip_prefix("ready ") {
        return Ok(url.to_owned());
    }
    let _ = child.wait();
    match line.strip_prefix("error ") {
        Some(message) => bail!("{message}"),
        None => bail!("viewer server exited before it was ready"),
    }
}

/// Reports the startup outcome to the waiting `open` process. Nothing is written afterwards,
/// because that process has exited and closed the pipe.
pub fn report_ready(url: &str) {
    report(&format!("ready {url}"));
}

pub fn report_failure(err: &anyhow::Error) {
    report(&format!("error {}", format!("{err:#}").replace('\n', " ")));
}

fn report(line: &str) {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

pub fn open_browser(url: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to run {opener}"))?;
    if !status.success() {
        bail!("{opener} could not open the browser ({status}); open {url} manually");
    }
    Ok(())
}

/// Built UI location: `HERDR_PLUGIN_ROOT/web/dist`, or relative to the working directory, which
/// Herdr sets to the plugin root for runtime commands.
pub fn dist_dir() -> Result<PathBuf> {
    let root = match env::var_os("HERDR_PLUGIN_ROOT").filter(|root| !root.is_empty()) {
        Some(root) => PathBuf::from(root),
        None => env::current_dir().context("failed to read the working directory")?,
    };
    Ok(root.join("web").join("dist"))
}

/// Server errors go to the plugin state directory; transcripts are never logged.
fn log_target() -> Stdio {
    let Some(dir) = env::var_os("HERDR_PLUGIN_STATE_DIR").filter(|dir| !dir.is_empty()) else {
        return Stdio::null();
    };
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(dir).join(LOG_FILE))
        .map_or_else(|_| Stdio::null(), Stdio::from)
}
