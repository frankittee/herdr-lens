//! Copies the viewer URL through the terminal with OSC 52.
//!
//! Plugin actions have no terminal, so the `copy` action opens a popup pane that runs `copy-pane`.
//! Herdr turns OSC 52 written by a pane into a clipboard write on the attached client, which also
//! works when that client is on another machine over SSH or `herdr --remote`.

use std::env;
use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};

pub const PANE_ENV: &str = "HERDR_LENS_PANE";
const PANE_ENTRYPOINT: &str = "copy";
const DEFAULT_PLUGIN_ID: &str = "herdr-lens";

/// Hands the resolved pane to the popup through an environment variable; the popup has no pane of its own.
pub fn open_popup(herdr: &crate::Herdr, pane_id: &str) -> Result<()> {
    let plugin = env::var("HERDR_PLUGIN_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| DEFAULT_PLUGIN_ID.to_owned());
    herdr.open_plugin_pane(&plugin, PANE_ENTRYPOINT, &format!("{PANE_ENV}={pane_id}"))
}

pub fn target_pane() -> Result<String> {
    env::var(PANE_ENV)
        .ok()
        .filter(|id| !id.is_empty())
        .with_context(|| format!("{PANE_ENV} is not set; use the Copy link action"))
}

/// Writes the URL to the clipboard via OSC 52 and shows it, then waits so the popup stays open.
pub fn show(url: &str) -> Result<()> {
    let mut out = io::stdout();
    write!(out, "{}", osc52(url))?;
    writeln!(out, "Herdr Lens: link copied\n\n{url}\n")?;
    if let Some(port) = port(url) {
        writeln!(
            out,
            "On a remote host, forward the port first:\n  ssh -L {port}:127.0.0.1:{port} <host>\n"
        )?;
    }
    wait_for_enter(&mut out)
}

pub fn show_error(err: &anyhow::Error) {
    let mut out = io::stdout();
    let _ = writeln!(out, "Herdr Lens: {err:#}\n");
    let _ = wait_for_enter(&mut out);
}

fn wait_for_enter(out: &mut impl Write) -> Result<()> {
    write!(out, "Press Enter to close")?;
    out.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(())
}

/// BEL terminator, as Herdr itself emits, because some terminals ignore ST-terminated OSC 52.
fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

fn port(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split('/').next()?;
    host.rsplit_once(':').map(|(_, port)| port)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn builds_bel_terminated_osc52() {
        assert_eq!(osc52("hello"), "\x1b]52;c;aGVsbG8=\x07");
    }

    #[test]
    fn extracts_port_from_url() {
        assert_eq!(port("http://127.0.0.1:12345/abc/pane/1"), Some("12345"));
        assert_eq!(port("http://localhost/abc"), None);
    }
}
