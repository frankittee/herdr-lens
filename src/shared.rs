//! Discovery record for the viewer started by Herdr's startup hook.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const STATE_FILE: &str = "viewer.json";

#[derive(Serialize, Deserialize)]
pub struct Address {
    pub port: u16,
    pub token: String,
    /// Process id and binary identity of the viewer, so `open` can replace one left running
    /// by an older build. Records written before these fields existed read as 0 and "".
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    build: String,
}

/// Identifies the running executable by size and modification time; a rebuild or upgrade changes it.
fn build_id() -> String {
    let modified = env::current_exe()
        .and_then(fs::metadata)
        .and_then(|meta| Ok((meta.len(), meta.modified()?)));
    match modified {
        Ok((len, time)) => {
            let nanos = time
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos());
            format!("{len}-{nanos}")
        }
        Err(_) => String::new(),
    }
}

/// Asks a stale viewer to exit; it is replaced right after, so a failure only leaves it idle.
fn stop(pid: u32) {
    if pid != 0 {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
}

fn path() -> Result<PathBuf> {
    let dir = env::var_os("HERDR_PLUGIN_STATE_DIR")
        .filter(|dir| !dir.is_empty())
        .context("HERDR_PLUGIN_STATE_DIR is required for the startup viewer")?;
    Ok(PathBuf::from(dir).join(STATE_FILE))
}

pub fn publish(port: u16, token: &str) -> Result<()> {
    let path = path()?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .context("failed to create viewer state")?;
    let address = Address {
        port,
        token: token.to_owned(),
        pid: std::process::id(),
        build: build_id(),
    };
    serde_json::to_writer(&mut file, &address).context("failed to encode viewer state")?;
    file.flush()?;
    fs::rename(temporary, path).context("failed to publish viewer state")
}

pub fn read() -> Result<Address> {
    let bytes = fs::read(path()?)
        .context("Lens is not ready; restart the Herdr server to run its startup hook")?;
    let address: Address = serde_json::from_slice(&bytes).context("invalid Lens viewer state")?;
    if address.token.len() != 32 || !address.token.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid Lens viewer token");
    }
    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, address.port);
    TcpStream::connect_timeout(&socket.into(), Duration::from_millis(300))
        .context("Lens viewer is not running; restart the Herdr server")?;
    if address.build != build_id() {
        stop(address.pid);
        bail!("Lens viewer was started by an older build");
    }
    Ok(address)
}
