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
    Ok(address)
}
