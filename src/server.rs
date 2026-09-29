//! Loopback HTTP server for the browser UI and its JSON API.
//!
//! Everything lives under a random token path, and requests must name the loopback host, so other
//! local users' browsers and DNS-rebinding pages cannot read the transcript. The server exits once
//! no request has arrived for `IDLE_TIMEOUT`; an open tab keeps it alive by polling.

use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::herdr::Herdr;
use crate::{agents, context, load};

/// Longer than the slowest background-tab polling browsers allow (about once a minute).
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TOKEN_BYTES: usize = 16;
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// The pane this server is pinned to. Pane ids can be reused, so the terminal id is checked too.
pub struct Pin {
    pub pane_id: String,
    pub terminal_id: Option<String>,
}

pub struct Viewer {
    server: Server,
    port: u16,
    token: String,
    dist: PathBuf,
    pin: Pin,
    last_error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    AddSlash,
    File(PathBuf),
    Agents,
    Conversation,
    NotFound,
}

type Reply = Response<Cursor<Vec<u8>>>;

impl Viewer {
    pub fn bind(dist: PathBuf, pin: Pin) -> Result<Self> {
        ensure_built(&dist)?;
        let server = Server::http("127.0.0.1:0")
            .map_err(|err| anyhow!("failed to bind the viewer server on 127.0.0.1: {err}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .map(|addr| addr.port())
            .context("viewer server has no TCP address")?;
        Ok(Self {
            server,
            port,
            token: random_token()?,
            dist,
            pin,
            last_error: None,
        })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/{}/", self.port, self.token)
    }

    pub fn run(mut self, herdr: &Herdr) -> Result<()> {
        while let Some(request) = self.server.recv_timeout(IDLE_TIMEOUT)? {
            self.handle(herdr, request);
        }
        Ok(())
    }

    fn handle(&mut self, herdr: &Herdr, request: Request) {
        let reply = if !matches!(request.method(), Method::Get | Method::Head) {
            text(405, "method not allowed")
        } else if !host_allowed(host_header(&request), self.port) {
            text(403, "forbidden")
        } else {
            let route = route(request.url(), &self.token);
            self.reply(herdr, route)
        };
        // A closed tab or a reload mid-response is expected; there is nobody to report it to.
        let _ = request.respond(reply);
    }

    fn reply(&mut self, herdr: &Herdr, route: Route) -> Reply {
        match route {
            Route::AddSlash => redirect(&format!("/{}/", self.token)),
            Route::File(path) => self.file(&path),
            Route::Agents => self.api("api/agents", agents::list(herdr)),
            Route::Conversation => {
                let conversation = self.conversation(herdr);
                self.api("api/conversation", conversation)
            }
            Route::NotFound => text(404, "not found"),
        }
    }

    fn conversation(&self, herdr: &Herdr) -> Result<crate::conversation::Conversation> {
        let target = context::resolve_pane(herdr, &self.pin.pane_id)?;
        if self.pin.terminal_id.is_some() && target.pane.terminal_id != self.pin.terminal_id {
            bail!(
                "pane {} now runs a different terminal; open Herdr Lens again from that pane",
                self.pin.pane_id
            );
        }
        load::conversation(herdr, target)
    }

    fn api<T: serde::Serialize>(&mut self, name: &str, result: Result<T>) -> Reply {
        match result.and_then(|value| serde_json::to_vec(&value).map_err(Into::into)) {
            Ok(body) => {
                self.last_error = None;
                bytes(200, "application/json", body)
            }
            Err(err) => {
                let message = format!("{err:#}");
                // The UI polls every few seconds; log each distinct failure once.
                if self.last_error.as_deref() != Some(message.as_str()) {
                    eprintln!("herdr-lens serve: {name}: {message}");
                    self.last_error = Some(message.clone());
                }
                let body = json!({ "error": message }).to_string().into_bytes();
                bytes(502, "application/json", body)
            }
        }
    }

    fn file(&self, relative: &Path) -> Reply {
        let path = self.dist.join(relative);
        match fs::read(&path) {
            Ok(body) => {
                let reply = bytes(200, content_type(&path), body);
                if relative == Path::new("index.html") {
                    reply.with_header(header("Content-Security-Policy", CONTENT_SECURITY_POLICY))
                } else {
                    reply
                }
            }
            Err(_) => text(404, "not found"),
        }
    }
}

fn ensure_built(dist: &Path) -> Result<()> {
    if !dist.join("index.html").is_file() {
        bail!(
            "web UI is not built: {} is missing; run `bun run --cwd web build`",
            dist.join("index.html").display()
        );
    }
    Ok(())
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .context("failed to read /dev/urandom")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn host_header(request: &Request) -> Option<&str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.as_str())
}

fn host_allowed(host: Option<&str>, port: u16) -> bool {
    host.is_some_and(|host| {
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

/// Maps a request URL to a route. Only plain relative file paths under `dist` are served.
fn route(url: &str, token: &str) -> Route {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let Some(rest) = path.strip_prefix('/').and_then(|p| p.strip_prefix(token)) else {
        return Route::NotFound;
    };
    if rest.is_empty() {
        return Route::AddSlash;
    }
    let Some(rest) = rest.strip_prefix('/') else {
        return Route::NotFound;
    };
    match rest {
        "" | "index.html" => Route::File(PathBuf::from("index.html")),
        "api/agents" => Route::Agents,
        "api/conversation" => Route::Conversation,
        _ => safe_relative(rest).map_or(Route::NotFound, Route::File),
    }
}

fn safe_relative(path: &str) -> Option<PathBuf> {
    if path.contains(['\\', '%']) {
        return None;
    }
    let relative = PathBuf::from(path);
    relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then_some(relative)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid ASCII")
}

fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Reply {
    Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        // The token is in the URL; never leak it to sites linked from a transcript.
        .with_header(header("Referrer-Policy", "no-referrer"))
}

fn text(status: u16, body: &str) -> Reply {
    bytes(
        status,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}

fn redirect(location: &str) -> Reply {
    text(308, "redirect").with_header(header("Location", location))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "abc123";

    #[test]
    fn routes_only_under_the_token() {
        assert_eq!(route("/abc123", TOKEN), Route::AddSlash);
        assert_eq!(route("/abc123/", TOKEN), Route::File("index.html".into()));
        assert_eq!(
            route("/abc123/?x=1", TOKEN),
            Route::File("index.html".into())
        );
        assert_eq!(route("/abc123/api/agents", TOKEN), Route::Agents);
        assert_eq!(
            route("/abc123/api/conversation", TOKEN),
            Route::Conversation
        );
        assert_eq!(
            route("/abc123/assets/index-1.js", TOKEN),
            Route::File("assets/index-1.js".into())
        );
        assert_eq!(route("/", TOKEN), Route::NotFound);
        assert_eq!(route("/api/agents", TOKEN), Route::NotFound);
        assert_eq!(route("/abc1234/", TOKEN), Route::NotFound);
    }

    #[test]
    fn rejects_paths_outside_dist() {
        assert_eq!(route("/abc123/../secret", TOKEN), Route::NotFound);
        assert_eq!(route("/abc123/assets/../../x", TOKEN), Route::NotFound);
        assert_eq!(route("/abc123//etc/passwd", TOKEN), Route::NotFound);
        assert_eq!(route("/abc123/%2e%2e/x", TOKEN), Route::NotFound);
        assert_eq!(route("/abc123/a\\b", TOKEN), Route::NotFound);
    }

    #[test]
    fn accepts_only_loopback_hosts_on_our_port() {
        assert!(host_allowed(Some("127.0.0.1:4000"), 4000));
        assert!(host_allowed(Some("localhost:4000"), 4000));
        assert!(!host_allowed(Some("127.0.0.1:4001"), 4000));
        assert!(!host_allowed(Some("evil.example:4000"), 4000));
        assert!(!host_allowed(None, 4000));
    }

    #[test]
    fn tokens_are_random_hex() {
        let a = random_token().unwrap();
        let b = random_token().unwrap();
        assert_eq!(a.len(), TOKEN_BYTES * 2);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
