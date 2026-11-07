//! The web view: the same live session, in a browser.
//!
//! The terminal owns the session; this module only serves it. A small HTTP
//! server (std threads, no async runtime) on 127.0.0.1, or with `--lan` on
//! every interface so other machines on the network can open it:
//!
//! ```text
//! GET  /?t=<token>       trade the session token for a cookie, redirect to /
//! GET  /                 the page (index.html, app.css, app.js are embedded)
//! GET  /events           Server-Sent Events: hello, state, out, toast, end
//! GET  /api/log?sid=<n>  download a terminal's run log
//! POST /api/action       one action as JSON, handed to the app's event loop
//! ```
//!
//! Every request must address the server by IP (or `localhost`), which
//! blocks DNS rebinding; locally only loopback addresses are accepted.
//! Everything except the token exchange needs the session cookie, and POSTs
//! must also come from the page's own origin, as JSON (CSRF guard). The
//! browser can only send the actions the terminal's keys trigger; it never
//! sends a command line, and only logs the app registered can be downloaded.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::Deserialize;

use crate::runner::AppEvent;

/// Port tried first; if it's taken a free one is picked (unless `--port`
/// asked for a specific one).
pub const DEFAULT_PORT: u16 = 7878;

/// Output lines kept per terminal for browsers that connect later (same as
/// the terminal's own in-memory buffer).
const MAX_LINES: usize = 2000;
const MAX_HEAD: u64 = 16 * 1024;
const MAX_BODY: usize = 64 * 1024;
/// Keep-alive comment on idle event streams; also how fast a closed tab is
/// noticed and stops counting as a viewer.
const PING: Duration = Duration::from_secs(5);

pub const INDEX_HTML: &str = include_str!("web/index.html");
pub const APP_CSS: &str = include_str!("web/app.css");
pub const APP_JS: &str = include_str!("web/app.js");

/// What a browser can ask for. Mirrors `app::Action`: tools are named
/// instead of indexed, and `term` is a terminal position (0 = console,
/// 1..=4 = tool terminals).
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebAction {
    /// `slot` (1..=4) picks the terminal to replace when all are busy.
    Run {
        tool: String,
        args: String,
        #[serde(default)]
        slot: Option<usize>,
    },
    Install {
        tool: String,
    },
    Cancel {
        term: usize,
    },
    Stdin {
        term: usize,
        line: String,
    },
    Eof {
        term: usize,
    },
    Close {
        term: usize,
    },
    Redetect,
    Hashid {
        hash: String,
    },
}

/// State shared between the app loop and the connection threads.
struct Hub {
    /// Static data sent once per connection (catalog, version).
    hello: String,
    /// The latest app state.
    state: String,
    /// Recent output of every open terminal, by session id.
    terms: BTreeMap<u64, VecDeque<String>>,
    /// Run logs that may be downloaded, by session id.
    logs: BTreeMap<u64, PathBuf>,
    /// One sender per open event stream.
    clients: Vec<Sender<String>>,
    ended: bool,
}

impl Hub {
    fn broadcast(&mut self, frame: &str) {
        self.clients.retain(|c| c.send(frame.to_string()).is_ok());
    }

    /// Everything a browser needs on (re)connect: it replaces what it had.
    fn snapshot(&self) -> String {
        let mut s = String::from("retry: 2000\n\n");
        s.push_str(&frame("hello", &self.hello));
        s.push_str(&frame("state", &self.state));
        for (sid, lines) in &self.terms {
            s.push_str(&out_frame(*sid, true, lines.iter()));
        }
        if self.ended {
            s.push_str(&frame("end", "{}"));
        }
        s
    }
}

/// One Server-Sent Events frame. `data` must be a single line (JSON is).
fn frame(event: &str, data: &str) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

/// An `out` frame: `{"sid": n, "reset": bool, "lines": [...]}`. With
/// `reset` the lines replace that terminal's output.
fn out_frame<'a>(sid: u64, reset: bool, lines: impl Iterator<Item = &'a String>) -> String {
    let data = serde_json::json!({
        "sid": sid,
        "reset": reset,
        "lines": lines.collect::<Vec<_>>(),
    });
    frame("out", &data.to_string())
}

struct Ctx {
    port: u16,
    /// Serving the LAN: any IP address of this machine is a valid Host.
    lan: bool,
    token: String,
    hub: Arc<Mutex<Hub>>,
    tx: Sender<AppEvent>,
}

impl Ctx {
    fn cookie_name(&self) -> String {
        format!("winsploit_{}", self.port)
    }
}

fn lock(hub: &Mutex<Hub>) -> MutexGuard<'_, Hub> {
    // a panicking connection thread must not take the web view down
    hub.lock().unwrap_or_else(|e| e.into_inner())
}

/// A running web view. Dropping it ends the session for every browser.
pub struct Server {
    port: u16,
    lan: bool,
    token: String,
    hub: Arc<Mutex<Hub>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    /// Start serving. `port`: exactly that port, or `None` for DEFAULT_PORT
    /// with a fallback to any free one. `lan`: listen on every interface
    /// instead of loopback only.
    pub fn start(
        port: Option<u16>,
        lan: bool,
        hello: String,
        tx: Sender<AppEvent>,
    ) -> std::io::Result<Server> {
        let host = if lan { "0.0.0.0" } else { "127.0.0.1" };
        let listener = match port {
            Some(p) => TcpListener::bind((host, p))?,
            None => {
                TcpListener::bind((host, DEFAULT_PORT)).or_else(|_| TcpListener::bind((host, 0)))?
            }
        };
        let port = listener.local_addr()?.port();
        let token = new_token()?;
        let hub = Arc::new(Mutex::new(Hub {
            hello,
            state: "{}".into(),
            terms: BTreeMap::new(),
            logs: BTreeMap::new(),
            clients: Vec::new(),
            ended: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Ctx {
            port,
            lan,
            token: token.clone(),
            hub: hub.clone(),
            tx,
        });
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("web-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let ctx = ctx.clone();
                    let _ = std::thread::Builder::new()
                        .name("web-conn".into())
                        .spawn(move || handle(stream, &ctx));
                }
            })?;
        Ok(Server {
            port,
            lan,
            token,
            hub,
            stop,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn lan(&self) -> bool {
        self.lan
    }

    /// The address that signs a browser in on this machine.
    pub fn login_url(&self) -> String {
        format!("http://127.0.0.1:{}/?t={}", self.port, self.token)
    }

    /// The address that signs in another machine on the network (LAN mode,
    /// when this machine's LAN address can be found).
    pub fn lan_url(&self) -> Option<String> {
        if !self.lan {
            return None;
        }
        let ip = lan_ip()?;
        Some(format!("http://{ip}:{}/?t={}", self.port, self.token))
    }

    /// Open event streams (browser tabs).
    pub fn viewers(&self) -> usize {
        lock(&self.hub).clients.len()
    }

    /// Publish a new app state (the caller only sends it when it changed).
    pub fn set_state(&self, json: String) {
        let mut hub = lock(&self.hub);
        let f = frame("state", &json);
        hub.state = json;
        hub.broadcast(&f);
    }

    /// Output for terminal `sid`: appended, or (with `reset`) replacing what
    /// it had, e.g. a new tool in that terminal.
    pub fn push_lines(&self, sid: u64, reset: bool, new: Vec<String>) {
        if new.is_empty() && !reset {
            return;
        }
        let mut hub = lock(&self.hub);
        let f = out_frame(sid, reset, new.iter());
        let buf = hub.terms.entry(sid).or_default();
        if reset {
            buf.clear();
        }
        buf.extend(new);
        while buf.len() > MAX_LINES {
            buf.pop_front();
        }
        hub.broadcast(&f);
    }

    /// Forget terminals that were closed or replaced: their output and logs
    /// stop being served.
    pub fn retain_terms(&self, open: &[u64]) {
        let mut hub = lock(&self.hub);
        hub.terms.retain(|sid, _| open.contains(sid));
        hub.logs.retain(|sid, _| open.contains(sid));
    }

    /// Terminal `sid`'s run log may be downloaded.
    pub fn set_log(&self, sid: u64, path: PathBuf) {
        lock(&self.hub).logs.insert(sid, path);
    }

    /// A corner notification (`kind`: ok / err / info).
    pub fn toast(&self, kind: &str, text: &str) {
        let data = serde_json::json!({ "kind": kind, "text": text });
        lock(&self.hub).broadcast(&frame("toast", &data.to_string()));
    }

    fn shutdown(&self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        {
            let mut hub = lock(&self.hub);
            hub.ended = true;
            hub.broadcast(&frame("end", "{}"));
            // dropping the senders ends every event stream
            hub.clients.clear();
        }
        // wake the accept loop so it sees the stop flag
        let _ = TcpStream::connect_timeout(
            &([127, 0, 0, 1], self.port).into(),
            Duration::from_millis(200),
        );
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// This machine's address on the network it routes through. Connecting a
/// UDP socket sends nothing; it only picks the outgoing interface.
fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect(("192.0.2.1", 9)).ok()?; // TEST-NET-1, never actually contacted
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// 128 random bits from the OS, as hex (short enough to copy from a
/// terminal without wrapping).
fn new_token() -> std::io::Result<String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Compare without bailing out at the first differing byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

// ------------------------------------------------------------------ HTTP

#[derive(Debug, Default)]
struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn query_param(&self, name: &str) -> Option<&str> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == name).then_some(v)
        })
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').find_map(|kv| {
            let (k, v) = kv.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }
}

/// Read one request: head capped at MAX_HEAD, body at MAX_BODY.
fn read_request(stream: &TcpStream) -> std::io::Result<Request> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let mut reader = BufReader::new(stream.take(MAX_HEAD));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| bad("empty request"))?
        .to_string();
    let target = parts.next().ok_or_else(|| bad("no target"))?.to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let mut headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(bad("headers cut short"));
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut req = Request {
        method,
        path,
        query,
        headers,
        body: Vec::new(),
    };
    let len: usize = req
        .header("content-length")
        .map(|v| v.parse().map_err(|_| bad("bad content-length")))
        .transpose()?
        .unwrap_or(0);
    if len > MAX_BODY {
        return Err(bad("body too large"));
    }
    // the head reader may already hold part of the body
    let mut body = vec![0u8; len];
    let buffered = reader.buffer().len().min(len);
    body[..buffered].copy_from_slice(&reader.buffer()[..buffered]);
    reader.consume(buffered);
    let mut rest = reader.into_inner().into_inner();
    rest.read_exact(&mut body[buffered..])?;
    req.body = body;
    Ok(req)
}

const SECURITY_HEADERS: &str = "Cache-Control: no-store\r\n\
X-Content-Type-Options: nosniff\r\n\
X-Frame-Options: DENY\r\n\
Referrer-Policy: no-referrer\r\n\
Cross-Origin-Opener-Policy: same-origin\r\n\
Cross-Origin-Resource-Policy: same-origin\r\n\
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; \
connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'\r\n";

fn respond(
    stream: &mut TcpStream,
    status: &str,
    extra: &[(&str, String)],
    content_type: &str,
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Connection: close\r\n{SECURITY_HEADERS}",
        body.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(body))
        .and_then(|_| stream.flush());
}

/// A small self-contained page for errors (no script).
fn message_page(title: &str, text: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=en><head><meta charset=utf-8>\
         <meta name=viewport content=\"width=device-width, initial-scale=1\">\
         <title>Winsploit</title><link rel=stylesheet href=/app.css></head>\
         <body class=message><main><div class=mark>winsploit</div><h1>{title}</h1>\
         <p>{text}</p></main></body></html>"
    )
    .into_bytes()
}

fn handle(mut stream: TcpStream, ctx: &Ctx) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(req) = read_request(&stream) else {
        respond(
            &mut stream,
            "400 Bad Request",
            &[],
            "text/plain",
            b"bad request",
        );
        return;
    };
    let _ = stream.set_read_timeout(None);
    if let Some((code, ctype, body)) = route(&mut stream, &req, ctx) {
        respond(&mut stream, code, &[], ctype, &body);
    }
    let _ = stream.shutdown(Shutdown::Both);
}

type Reply = Option<(&'static str, &'static str, Vec<u8>)>;

/// Answer `req`. Returns a simple reply to send, or None when the handler
/// already wrote its own response (redirects, downloads, event streams).
fn route(stream: &mut TcpStream, req: &Request, ctx: &Ctx) -> Reply {
    let html = "text/html; charset=utf-8";
    if !host_ok(req.header("host"), ctx.port, ctx.lan) {
        return Some((
            "421 Misdirected Request",
            "text/plain",
            b"wrong host".to_vec(),
        ));
    }
    let authed = req
        .cookie(&ctx.cookie_name())
        .is_some_and(|c| same(c, &ctx.token));

    match (req.method.as_str(), req.path.as_str()) {
        // the stylesheet is needed by the sign-in error pages too
        ("GET", "/app.css") => Some(("200 OK", "text/css; charset=utf-8", APP_CSS.into())),
        ("GET", "/") if req.query_param("t").is_some() => {
            if req.query_param("t").is_some_and(|t| same(t, &ctx.token)) {
                let cookie = format!(
                    "{}={}; HttpOnly; SameSite=Strict; Path=/",
                    ctx.cookie_name(),
                    ctx.token
                );
                respond(
                    stream,
                    "303 See Other",
                    &[("Location", "/".into()), ("Set-Cookie", cookie)],
                    "text/plain",
                    b"",
                );
                None
            } else {
                Some((
                    "403 Forbidden",
                    html,
                    message_page(
                        "This link has expired",
                        "Each Winsploit session has its own link. Press <b>o</b> in the \
                         terminal to open the current one.",
                    ),
                ))
            }
        }
        _ if !authed => Some((
            "401 Unauthorized",
            html,
            message_page(
                "Open Winsploit from the terminal",
                "The web view only accepts the link printed in your terminal. Press \
                 <b>o</b> in Winsploit to open it here.",
            ),
        )),
        ("GET", "/") => Some(("200 OK", html, INDEX_HTML.into())),
        ("GET", "/app.js") => Some(("200 OK", "text/javascript; charset=utf-8", APP_JS.into())),
        ("GET", "/favicon.svg") => Some(("200 OK", "image/svg+xml", FAVICON.into())),
        ("GET", "/events") => {
            serve_events(stream, ctx);
            None
        }
        ("GET", "/api/log") => serve_log(stream, req, ctx),
        ("POST", "/api/action") => Some(post_action(req, ctx)),
        _ => Some(("404 Not Found", "text/plain", b"not found".to_vec())),
    }
}

const FAVICON: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'>\
<rect width='32' height='32' rx='8' fill='#d77757'/>\
<path d='M7 10l3.2 12 3-8 2.8 8 3.2-12' fill='none' stroke='#fff' stroke-width='2.6' \
stroke-linecap='round' stroke-linejoin='round'/><circle cx='24' cy='21' r='2.2' fill='#fff'/></svg>";

/// The Host header must be `<address>:<our port>`, where the address is an
/// IP literal or `localhost`: a DNS-rebinding page (evil.example resolving
/// to us) sends its own name and is refused. Locally only loopback counts.
fn host_ok(host: Option<&str>, port: u16, lan: bool) -> bool {
    let Some((name, p)) = host.and_then(|h| h.rsplit_once(':')) else {
        return false;
    };
    if p != port.to_string() {
        return false;
    }
    if name.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let ip = name.trim_start_matches('[').trim_end_matches(']');
    match ip.parse::<IpAddr>() {
        Ok(ip) => lan || ip.is_loopback(),
        Err(_) => false,
    }
}

/// State-changing requests must come from our own page: an Origin equal to
/// the (valid) Host it was sent to, and a JSON body (a cross-site form can't
/// send that without a CORS preflight, which is never answered).
fn origin_ok(req: &Request, port: u16, lan: bool) -> bool {
    let host = req.header("host");
    let origin = host.is_some_and(|h| {
        host_ok(Some(h), port, lan) && req.header("origin") == Some(&format!("http://{h}"))
    });
    let json = req
        .header("content-type")
        .is_some_and(|c| c.trim_start().starts_with("application/json"));
    let fetch_site = req
        .header("sec-fetch-site")
        .is_none_or(|s| s == "same-origin");
    origin && json && fetch_site
}

fn post_action(req: &Request, ctx: &Ctx) -> (&'static str, &'static str, Vec<u8>) {
    if !origin_ok(req, ctx.port, ctx.lan) {
        return (
            "403 Forbidden",
            "text/plain",
            b"cross-origin request".to_vec(),
        );
    }
    match serde_json::from_slice::<WebAction>(&req.body) {
        Ok(action) => {
            if ctx.tx.send(AppEvent::Web(action)).is_err() {
                return (
                    "503 Service Unavailable",
                    "text/plain",
                    b"session ended".to_vec(),
                );
            }
            ("202 Accepted", "application/json", b"{}".to_vec())
        }
        Err(e) => (
            "400 Bad Request",
            "text/plain",
            format!("bad action: {e}").into_bytes(),
        ),
    }
}

/// A terminal's run log, as a download. Only paths the app registered.
fn serve_log(stream: &mut TcpStream, req: &Request, ctx: &Ctx) -> Reply {
    let sid = req.query_param("sid").and_then(|s| s.parse::<u64>().ok());
    let path = sid.and_then(|sid| lock(&ctx.hub).logs.get(&sid).cloned());
    let Some(path) = path else {
        return Some((
            "404 Not Found",
            "text/plain",
            b"no log for that terminal".to_vec(),
        ));
    };
    let Ok(body) = std::fs::read(&path) else {
        return Some((
            "404 Not Found",
            "text/plain",
            b"the log file is gone".to_vec(),
        ));
    };
    // run log names are already sanitized; keep the header safe regardless
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "run.log".into())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
        .collect();
    respond(
        stream,
        "200 OK",
        &[(
            "Content-Disposition",
            format!("attachment; filename=\"{name}\""),
        )],
        "text/plain; charset=utf-8",
        &body,
    );
    None
}

fn serve_events(stream: &mut TcpStream, ctx: &Ctx) {
    let (tx, rx) = mpsc::channel::<String>();
    let snapshot = {
        let mut hub = lock(&ctx.hub);
        if !hub.ended {
            hub.clients.push(tx);
        }
        hub.snapshot()
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
         Connection: close\r\n{SECURITY_HEADERS}\r\n"
    );
    let mut send = |s: &str| stream.write_all(s.as_bytes()).and_then(|_| stream.flush());
    if send(&head).and_then(|_| send(&snapshot)).is_err() {
        return;
    }
    // Runs until the tab goes away (a write fails) or the server stops: it
    // queues an `end` frame and then drops every sender, so the frames still
    // queued (that `end` included) are written before Disconnected.
    loop {
        let res = match rx.recv_timeout(PING) {
            Ok(f) => send(&f),
            Err(RecvTimeoutError::Timeout) => send(": ping\n\n"),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if res.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(headers: &[(&str, &str)]) -> Request {
        Request {
            method: "POST".into(),
            path: "/api/action".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn host_must_be_an_address_of_ours() {
        let ok = |h: &str, lan| host_ok(Some(h), 7878, lan);
        assert!(ok("127.0.0.1:7878", false));
        assert!(ok("localhost:7878", false));
        assert!(ok("[::1]:7878", false));
        // a name that resolves to us is exactly what DNS rebinding sends
        assert!(!ok("evil.example:7878", false));
        assert!(!ok("evil.example:7878", true));
        assert!(!ok("127.0.0.1:9999", false));
        // LAN addresses only count in LAN mode
        assert!(!ok("192.168.1.23:7878", false));
        assert!(ok("192.168.1.23:7878", true));
        assert!(!host_ok(None, 7878, true));
    }

    #[test]
    fn posts_need_our_origin_and_json() {
        let good = [
            ("Host", "127.0.0.1:7878"),
            ("Origin", "http://127.0.0.1:7878"),
            ("Content-Type", "application/json"),
        ];
        assert!(origin_ok(&req(&good), 7878, false));
        let with = |k: &str, v: &str| {
            let mut h: Vec<(&str, &str)> = good.iter().filter(|(n, _)| *n != k).cloned().collect();
            h.push((k, v));
            req(&h.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>())
        };
        assert!(!origin_ok(
            &with("Origin", "http://evil.example"),
            7878,
            false
        ));
        assert!(!origin_ok(&with("Content-Type", "text/plain"), 7878, false));
        assert!(!origin_ok(
            &with("Sec-Fetch-Site", "cross-site"),
            7878,
            false
        ));
        // the Origin must match the Host the request was sent to
        assert!(!origin_ok(
            &with("Origin", "http://localhost:7878"),
            7878,
            false
        ));
        // on the LAN: our LAN address, in LAN mode only
        let lan = [
            ("Host", "192.168.1.23:7878"),
            ("Origin", "http://192.168.1.23:7878"),
            ("Content-Type", "application/json"),
        ];
        assert!(origin_ok(&req(&lan), 7878, true));
        assert!(!origin_ok(&req(&lan), 7878, false));
    }

    #[test]
    fn cookie_and_query_parsing() {
        let mut r = req(&[("Cookie", "a=1; winsploit_7878=abc ; b=2")]);
        assert_eq!(r.cookie("winsploit_7878"), Some("abc"));
        assert_eq!(r.cookie("winsploit_1"), None);
        r.query = "x=1&t=tok".into();
        assert_eq!(r.query_param("t"), Some("tok"));
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }

    #[test]
    fn actions_parse_from_json() {
        let a: WebAction =
            serde_json::from_str(r#"{"type":"run","tool":"nmap","args":"-sV x"}"#).unwrap();
        assert_eq!(
            a,
            WebAction::Run {
                tool: "nmap".into(),
                args: "-sV x".into(),
                slot: None
            }
        );
        assert_eq!(
            serde_json::from_str::<WebAction>(r#"{"type":"eof","term":2}"#).unwrap(),
            WebAction::Eof { term: 2 }
        );
        assert!(serde_json::from_str::<WebAction>(r#"{"type":"shell","cmd":"x"}"#).is_err());
    }

    #[test]
    fn snapshot_has_every_terminal() {
        let mut terms = BTreeMap::new();
        terms.insert(0, VecDeque::from(vec!["console line".to_string()]));
        terms.insert(3, VecDeque::from(vec!["nmap line".to_string()]));
        let h = Hub {
            hello: "{}".into(),
            state: "{}".into(),
            terms,
            logs: BTreeMap::new(),
            clients: vec![],
            ended: false,
        };
        let s = h.snapshot();
        assert!(s.contains(r#""sid":0"#) && s.contains("console line"));
        assert!(s.contains(r#""sid":3"#) && s.contains("nmap line"));
        assert!(s.contains(r#""reset":true"#));
    }

    /// End to end over a real socket: sign in, stream, act, download, and
    /// the guards.
    #[test]
    fn serves_signs_in_streams_and_guards() {
        let (tx, rx) = mpsc::channel();
        let srv = Server::start(Some(0), false, r#"{"hi":1}"#.into(), tx).unwrap();
        srv.push_lines(0, true, vec!["first".into()]);
        let port = srv.port();
        let host = format!("127.0.0.1:{port}");
        let raw = |text: String| -> String {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(text.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        };
        let get = |path: &str, extra: &str| {
            raw(format!(
                "GET {path} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n"
            ))
        };
        // no cookie, wrong token, wrong host
        assert!(get("/", "").starts_with("HTTP/1.1 401"));
        assert!(get("/?t=nope", "").starts_with("HTTP/1.1 403"));
        assert!(
            raw("GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n".into()).starts_with("HTTP/1.1 421")
        );

        // the right token sets the cookie
        assert!(get(&format!("/?t={}", srv.token), "").starts_with("HTTP/1.1 303"));
        let cookie = format!("Cookie: winsploit_{port}={}\r\n", srv.token);
        assert!(get("/", &cookie).contains("<!doctype html>"));

        // an action from our origin reaches the app loop
        let body = r#"{"type":"cancel","term":1}"#;
        let post = |origin: &str| {
            raw(format!(
                "POST /api/action HTTP/1.1\r\nHost: {host}\r\n{cookie}Origin: {origin}\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ))
        };
        assert!(post(&format!("http://{host}")).starts_with("HTTP/1.1 202"));
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(AppEvent::Web(WebAction::Cancel { term: 1 })) => {}
            _ => panic!("action didn't arrive"),
        }
        assert!(post("http://evil.example").starts_with("HTTP/1.1 403"));

        // logs: only registered ones
        let dir = std::env::temp_dir().join(format!("winsploit-web-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("20260101-000000Z-nmap.log");
        std::fs::write(&log, "# winsploit run log\nPORT STATE\n").unwrap();
        assert!(get("/api/log?sid=5", &cookie).starts_with("HTTP/1.1 404"));
        srv.set_log(5, log.clone());
        let dl = get("/api/log?sid=5", &cookie);
        assert!(dl.starts_with("HTTP/1.1 200") && dl.contains("PORT STATE"));
        assert!(dl.contains("filename=\"20260101-000000Z-nmap.log\""));
        assert!(get("/api/log?sid=5", "").starts_with("HTTP/1.1 401"));
        srv.retain_terms(&[0]);
        assert!(get("/api/log?sid=5", &cookie).starts_with("HTTP/1.1 404"));
        let _ = std::fs::remove_dir_all(&dir);

        // the event stream starts with a snapshot and follows new output
        let mut es = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(es, "GET /events HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\n").unwrap();
        es.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut seen = String::new();
        let mut buf = [0u8; 4096];
        let mut read_until = |needle: &str, seen: &mut String| {
            while !seen.contains(needle) {
                let n = es.read(&mut buf).expect("stream stalled");
                assert!(n > 0, "stream closed before {needle:?}");
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        };
        read_until("first", &mut seen);
        assert!(seen.contains("event: hello") && seen.contains("event: state"));
        srv.push_lines(2, false, vec!["second".into()]);
        read_until("second", &mut seen);
        srv.toast("ok", "nmap finished");
        read_until("nmap finished", &mut seen);
        drop(srv);
        read_until("event: end", &mut seen);
    }

    #[test]
    fn lan_mode_listens_beyond_loopback() {
        let (tx, _rx) = mpsc::channel();
        let srv = Server::start(Some(0), true, "{}".into(), tx).unwrap();
        assert!(srv.lan());
        // reachable on loopback too (0.0.0.0 covers it)
        assert!(TcpStream::connect(("127.0.0.1", srv.port())).is_ok());
        if let Some(url) = srv.lan_url() {
            assert!(url.starts_with("http://") && url.contains(&format!(":{}/?t=", srv.port())));
        }
    }
}
