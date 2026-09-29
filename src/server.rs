//! Loopback HTTP server for the browser UI and its JSON API.
//!
//! Everything lives under a random token path, and requests must name the loopback host, so other
//! local users' browsers and DNS-rebinding pages cannot read the transcript. Standalone pane
//! viewers exit after `IDLE_TIMEOUT`; the shared startup viewer runs while Herdr's socket exists.

use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::herdr::{AgentInfo, Herdr};
use crate::{agents, context, load};

/// Longer than the slowest background-tab polling browsers allow (about once a minute).
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SHARED_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const TOKEN_BYTES: usize = 16;
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// A selected agent. Pane ids can be reused, so terminal and agent identity are checked too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub pane_id: String,
    pub terminal_id: Option<String>,
    pub agent: String,
}

pub struct Viewer {
    server: Server,
    port: u16,
    token: String,
    dist: PathBuf,
    pin: Option<Pin>,
    last_error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    AddSlash,
    File(PathBuf),
    Agents,
    Conversation(Option<Pin>),
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
            pin: Some(pin),
            last_error: None,
        })
    }

    pub fn bind_shared(dist: PathBuf) -> Result<Self> {
        ensure_built(&dist)?;
        let server = Server::http("127.0.0.1:0")
            .map_err(|err| anyhow!("failed to bind the viewer server on 127.0.0.1: {err}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .context("viewer server has no TCP address")?
            .port();
        Ok(Self {
            server,
            port,
            token: random_token()?,
            dist,
            pin: None,
            last_error: None,
        })
    }

    pub fn address(&self) -> (u16, &str) {
        (self.port, &self.token)
    }

    pub fn url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/{}/{}/",
            self.port,
            self.token,
            pane_slug(&self.pin.as_ref().expect("pinned viewer").pane_id)
        )
    }

    pub fn run(mut self, herdr: &Herdr) -> Result<()> {
        let socket = std::env::var_os("HERDR_SOCKET_PATH")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let socket_identity = socket
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .map(|metadata| (metadata.dev(), metadata.ino()));
        loop {
            let timeout = if self.pin.is_some() {
                IDLE_TIMEOUT
            } else {
                SHARED_CHECK_INTERVAL
            };
            match self.server.recv_timeout(timeout)? {
                Some(request) => self.handle(herdr, request),
                None if self.pin.is_some() => return Ok(()),
                None => {
                    if let (Some(path), Some(identity)) = (&socket, socket_identity) {
                        let current = fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()));
                        if current != Some(identity) {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    fn handle(&mut self, herdr: &Herdr, request: Request) {
        let reply = if !matches!(request.method(), Method::Get | Method::Head) {
            text(405, "method not allowed")
        } else if !host_allowed(host_header(&request), self.port) {
            text(403, "forbidden")
        } else {
            let route = route(request.url(), &self.token);
            self.reply(herdr, route, request.url())
        };
        // A closed tab or a reload mid-response is expected; there is nobody to report it to.
        let _ = request.respond(reply);
    }

    fn reply(&mut self, herdr: &Herdr, route: Route, url: &str) -> Reply {
        match route {
            Route::AddSlash => {
                let path = url.split(['?', '#']).next().unwrap_or("");
                redirect(&format!("{path}/"))
            }
            Route::File(path) => self.file(&path),
            Route::Agents => self.api("api/agents", agents::list(herdr)),
            Route::Conversation(selected) => {
                let pin = self
                    .pin
                    .as_ref()
                    .cloned()
                    .or_else(|| shared_pin(url, &self.token));
                let conversation = pin
                    .as_ref()
                    .ok_or_else(|| anyhow!("viewer URL has no pane identity"))
                    .and_then(|pin| self.conversation(herdr, pin, selected.as_ref()));
                self.api("api/conversation", conversation)
            }
            Route::NotFound => text(404, "not found"),
        }
    }

    fn conversation(
        &self,
        herdr: &Herdr,
        default: &Pin,
        selected: Option<&Pin>,
    ) -> Result<crate::conversation::Conversation> {
        let pin = selected.unwrap_or(default);
        if selected.is_some() && !selected_is_live(&herdr.agents()?, pin) {
            bail!("selected agent is no longer available in Herdr");
        }
        let target = context::resolve_pane(herdr, &pin.pane_id)?;
        if pin.terminal_id.is_some() && target.pane.terminal_id != pin.terminal_id {
            bail!(
                "pane {} now runs a different terminal; open Herdr Lens again from that pane",
                pin.pane_id
            );
        }
        if target.agent != pin.agent {
            bail!("pane {} now runs a different agent", pin.pane_id);
        }
        if selected.is_some() && !agents::supports_conversation(&target.agent) {
            bail!("This harness is not supported yet");
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

fn selected_is_live(agents: &[AgentInfo], pin: &Pin) -> bool {
    agents.iter().any(|agent| {
        agent.pane_id == pin.pane_id
            && agent.terminal_id == pin.terminal_id
            && agent.agent == pin.agent
    })
}

fn ensure_built(dist: &Path) -> Result<()> {
    if !dist.join("index.html").is_file() {
        bail!(
            "web UI is not built: {} is missing; run `mise run build`",
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

fn pane_slug(pane_id: &str) -> String {
    pane_id.replace(':', "-")
}

fn valid_pane_slug(slug: &str) -> bool {
    let Some((workspace, pane)) = slug.split_once('-') else {
        return false;
    };
    workspace.len() > 1
        && workspace.starts_with('w')
        && workspace[1..].bytes().all(|c| c.is_ascii_alphanumeric())
        && pane.len() > 1
        && pane.starts_with('p')
        && pane[1..].bytes().all(|c| c.is_ascii_alphanumeric())
}

fn shared_pin(url: &str, token: &str) -> Option<Pin> {
    let path = url.split(['?', '#']).next()?;
    let rest = path.strip_prefix(&format!("/{token}/"))?;
    let mut parts = rest.split('/');
    let pane_id = parts.next()?;
    let terminal_id = parts.next()?;
    let agent = parts.next()?;
    if !pane_id.contains(':')
        || !valid_pane_slug(&pane_slug(pane_id))
        || ![pane_id, terminal_id, agent].iter().all(|id| safe_id(id))
    {
        return None;
    }
    Some(Pin {
        pane_id: pane_id.to_owned(),
        terminal_id: Some(terminal_id.to_owned()),
        agent: agent.to_owned(),
    })
}

pub fn shared_url(port: u16, token: &str, pin: &Pin) -> Result<String> {
    let terminal = pin
        .terminal_id
        .as_deref()
        .context("pane has no terminal identity")?;
    if ![pin.pane_id.as_str(), terminal, pin.agent.as_str()]
        .iter()
        .all(|id| safe_id(id))
    {
        bail!("pane identity contains unsupported URL characters");
    }
    Ok(format!(
        "http://127.0.0.1:{port}/{token}/{}/{terminal}/{}/",
        pin.pane_id, pin.agent
    ))
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
    let rest = if let Some(pin) = shared_pin(url, token) {
        let prefix = format!(
            "{}/{}/{}/",
            pin.pane_id,
            pin.terminal_id.unwrap(),
            pin.agent
        );
        rest.strip_prefix(&prefix).unwrap_or(rest)
    } else {
        rest
    };
    let rest = if let Some((slug, tail)) = rest.split_once('/') {
        if valid_pane_slug(slug) {
            tail
        } else {
            rest
        }
    } else if valid_pane_slug(rest) {
        return Route::AddSlash;
    } else {
        rest
    };
    match rest {
        "" | "index.html" => Route::File(PathBuf::from("index.html")),
        "api/agents" => Route::Agents,
        "api/conversation" => Route::Conversation(None),
        _ if rest.starts_with("api/conversation/") => {
            let ids = &rest["api/conversation/".len()..];
            match ids.split('/').collect::<Vec<_>>().as_slice() {
                [pane_id, terminal_id, agent]
                    if [*pane_id, *terminal_id, *agent]
                        .iter()
                        .all(|id| safe_id(id)) =>
                {
                    Route::Conversation(Some(Pin {
                        pane_id: (*pane_id).to_owned(),
                        terminal_id: Some((*terminal_id).to_owned()),
                        agent: (*agent).to_owned(),
                    }))
                }
                _ => Route::NotFound,
            }
        }
        _ => safe_relative(rest).map_or(Route::NotFound, Route::File),
    }
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'-'))
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
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
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
        assert_eq!(route("/abc123/w1-p2", TOKEN), Route::AddSlash);
        assert_eq!(
            route("/abc123/w1-p2/", TOKEN),
            Route::File("index.html".into())
        );
        assert_eq!(
            route("/abc123/wM-p1/", TOKEN),
            Route::File("index.html".into())
        );
        assert_eq!(route("/abc123/w1-p2/api/agents", TOKEN), Route::Agents);
        assert_eq!(
            route("/abc123/w1-p2/api/conversation", TOKEN),
            Route::Conversation(None)
        );
        assert_eq!(
            route("/abc123/w1-p2/assets/index-1.js", TOKEN),
            Route::File("assets/index-1.js".into())
        );
        assert_eq!(route("/abc123/", TOKEN), Route::File("index.html".into()));
        assert_eq!(
            route("/abc123/?x=1", TOKEN),
            Route::File("index.html".into())
        );
        assert_eq!(route("/abc123/api/agents", TOKEN), Route::Agents);
        assert_eq!(
            route("/abc123/api/conversation", TOKEN),
            Route::Conversation(None)
        );
        assert_eq!(
            route("/abc123/api/conversation/w1:p2/term_2/codex", TOKEN),
            Route::Conversation(Some(Pin {
                pane_id: "w1:p2".into(),
                terminal_id: Some("term_2".into()),
                agent: "codex".into(),
            }))
        );
        assert_eq!(
            route("/abc123/api/conversation/w1:p2/term_2", TOKEN),
            Route::NotFound
        );
        assert_eq!(
            route("/abc123/api/conversation/w1:p2/term_2/../x", TOKEN),
            Route::NotFound
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
    fn shared_urls_keep_the_invoking_pane_in_the_base_path() {
        let pin = Pin {
            pane_id: "w1:p2".into(),
            terminal_id: Some("term_2".into()),
            agent: "codex".into(),
        };
        let url = shared_url(4000, TOKEN, &pin).unwrap();
        assert_eq!(url, "http://127.0.0.1:4000/abc123/w1:p2/term_2/codex/");
        let path = "/abc123/w1:p2/term_2/codex/api/conversation";
        assert_eq!(shared_pin(path, TOKEN), Some(pin));
        assert_eq!(route(path, TOKEN), Route::Conversation(None));
        assert_eq!(
            route("/abc123/w1:p2/term_2/codex/assets/app.js", TOKEN),
            Route::File("assets/app.js".into())
        );
        assert_eq!(
            route("/abc123/w1:p2/term_2/codex/w1-p3/api/agents", TOKEN),
            Route::Agents
        );
        assert_eq!(
            shared_pin("/abc123/api/conversation/w1:p2/term_2/codex", TOKEN),
            None
        );
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

    #[test]
    fn selected_agent_must_match_live_pane_terminal_and_kind() {
        let agent = AgentInfo {
            pane_id: "w1:p2".into(),
            terminal_id: Some("term_2".into()),
            workspace_id: "w1".into(),
            tab_id: "w1:t1".into(),
            agent: "codex".into(),
            agent_status: "idle".into(),
            terminal_title_stripped: None,
            cwd: None,
            focused: false,
        };
        let mut selected = Pin {
            pane_id: "w1:p2".into(),
            terminal_id: Some("term_2".into()),
            agent: "codex".into(),
        };
        assert!(selected_is_live(std::slice::from_ref(&agent), &selected));
        selected.terminal_id = Some("term_old".into());
        assert!(!selected_is_live(std::slice::from_ref(&agent), &selected));
        selected.terminal_id = agent.terminal_id.clone();
        selected.agent = "claude".into();
        assert!(!selected_is_live(&[agent], &selected));
    }
}
