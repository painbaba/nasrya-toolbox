//! The web view: the same live session, in a browser.
//!
//! The terminal owns the session; this module only serves it. A small HTTP
//! server (std threads, no async runtime) on 127.0.0.1, or with `--lan` on
//! every interface so other machines on the network can open it:
//!
//! ```text
//! GET  /?t=<token>       owner sign-in (this machine only): token -> cookie
//! GET  /                 the page, or the join page for a guest not let in yet
//! GET  /events           Server-Sent Events: hello, you, state, out, toast, admin, end
//! GET  /api/log?sid=<n>  download a terminal's run log
//! POST /api/action       one action as JSON, handed to the app's event loop
//! POST /api/join         LAN guest: trade a one-time join key for a guest cookie
//! GET  /api/join         LAN guest: waiting / in / denied / removed
//! ```
//!
//! **Who is who.** The *owner* is whoever opened the token link on this
//! machine: the token is only accepted from a loopback address, so it never
//! has to cross the network. In LAN mode, other machines connect over
//! **HTTPS only** (one port: a TLS handshake gets TLS, plain HTTP from the
//! network is redirected) and become *guests*: they type a one-time join key
//! shown in the terminal, then wait until the owner lets them in, as
//! watch-only or with full control. Only the owner manages guests.
//!
//! Every request must address the server by IP (or `localhost`), which
//! blocks DNS rebinding; locally only loopback addresses are accepted.
//! POSTs must also come from the page's own origin, as JSON (CSRF guard).
//! The browser can only send the actions the terminal's keys trigger; it
//! never sends a command line, and only logs the app registered can be
//! downloaded.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::runner::AppEvent;
use crate::tls;

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

/// A join key: this many characters from KEY_ALPHABET (about 30 bits),
/// usable once, for KEY_LIFE, and burned after KEY_TRIES wrong guesses.
const KEY_LEN: usize = 6;
const KEY_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
pub const KEY_LIFE: Duration = Duration::from_secs(10 * 60);
pub const KEY_TRIES: u8 = 5;
/// Slows down guessing a little more (per wrong key, per connection).
const WRONG_KEY_DELAY: Duration = Duration::from_millis(400);
/// Longest guest name kept.
const NAME_MAX: usize = 24;

pub const INDEX_HTML: &str = include_str!("web/index.html");
pub const APP_CSS: &str = include_str!("web/app.css");
pub const APP_JS: &str = include_str!("web/app.js");
pub const JOIN_HTML: &str = include_str!("web/join.html");
pub const JOIN_JS: &str = include_str!("web/join.js");

/// What a guest may do once let in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// sees everything live, can't act
    Watch,
    /// everything the terminal's keys do, except managing guests
    Control,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Watch => "watch only",
            Role::Control => "full control",
        }
    }
}

/// Where a LAN guest stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestStatus {
    /// gave a valid key, waiting for the owner
    Waiting,
    In(Role),
    /// the owner said no while they were waiting
    Denied,
    /// the owner removed them after letting them in
    Removed,
}

/// A guest as the terminal shows it (never the secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestInfo {
    pub id: u64,
    /// what they typed (cleaned), may be empty
    pub name: String,
    pub ip: IpAddr,
    /// "Chrome on Windows"
    pub agent: String,
    pub status: GuestStatus,
    /// open event streams (tabs)
    pub online: usize,
}

impl GuestInfo {
    /// `"Taha" (192.168.1.23)`, or just the address.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.ip.to_string()
        } else {
            format!("\"{}\" ({})", self.name, self.ip)
        }
    }
}

/// Who sent an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// the token holder on this machine
    Owner,
    /// a LAN guest that was let in with full control
    Guest(u64),
}

/// Something the terminal should know about the LAN view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanEvent {
    /// guest `id` gave a valid key and waits to be let in
    JoinRequest(u64),
    /// too many wrong keys: the current key no longer works
    KeyBurned,
}

/// The join key as the terminal shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// `K7F-29Q`
    pub code: String,
    pub state: KeyState,
    pub expires: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Live,
    Used,
    Expired,
    Burned,
}

impl KeyState {
    pub fn label(self) -> &'static str {
        match self {
            KeyState::Live => "live",
            KeyState::Used => "used",
            KeyState::Expired => "expired",
            KeyState::Burned => "burned (too many wrong tries)",
        }
    }
}

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
    /// Switch to an existing challenge, by id (`<event>/<challenge>`).
    Challenge {
        id: String,
    },
    /// owner only: let a guest in (or change their role)
    Admit {
        guest: u64,
        role: Role,
    },
    /// owner only: deny a waiting guest or remove one who is in
    Remove {
        guest: u64,
    },
    /// owner only: a new join key
    Newkey,
}

impl WebAction {
    /// Guest management: only the owner may send these.
    pub fn owner_only(&self) -> bool {
        matches!(
            self,
            WebAction::Admit { .. } | WebAction::Remove { .. } | WebAction::Newkey
        )
    }
}

struct JoinKey {
    /// KEY_LEN characters, no dash
    code: String,
    expires: Instant,
    /// wall-clock expiry for the owner's page (unix ms)
    expires_ms: u64,
    misses: u8,
    used: bool,
}

impl JoinKey {
    fn state(&self, now: Instant) -> KeyState {
        if self.used {
            KeyState::Used
        } else if self.misses >= KEY_TRIES {
            KeyState::Burned
        } else if now >= self.expires {
            KeyState::Expired
        } else {
            KeyState::Live
        }
    }
}

struct Guest {
    info: GuestInfo,
    secret: String,
}

/// One open event stream and who it belongs to.
struct Client {
    who: Caller,
    tx: Sender<String>,
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
    clients: Vec<Client>,
    ended: bool,
    /// LAN mode: the current join key and everyone who used one.
    key: Option<JoinKey>,
    guests: Vec<Guest>,
    next_guest: u64,
    /// What the owner's page shows guests: the address and the certificate.
    url: Option<String>,
    fingerprint: Option<String>,
}

impl Hub {
    /// To every open stream.
    fn broadcast(&mut self, frame: &str) {
        self.send_where(frame, |_| true);
    }

    fn send_where(&mut self, frame: &str, to: impl Fn(Caller) -> bool) {
        self.clients
            .retain(|c| !to(c.who) || c.tx.send(frame.to_string()).is_ok());
    }

    fn guest(&self, id: u64) -> Option<&Guest> {
        self.guests.iter().find(|g| g.info.id == id)
    }

    fn guest_mut(&mut self, id: u64) -> Option<&mut Guest> {
        self.guests.iter_mut().find(|g| g.info.id == id)
    }

    fn online(&self, id: u64) -> usize {
        self.clients
            .iter()
            .filter(|c| c.who == Caller::Guest(id))
            .count()
    }

    /// The `you` frame: who this page is signed in as.
    fn you(&self, who: Caller) -> String {
        let data = match who {
            Caller::Owner => serde_json::json!({ "role": "owner" }),
            Caller::Guest(id) => {
                let g = self.guest(id);
                let role = match g.map(|g| g.info.status) {
                    Some(GuestStatus::In(r)) => Some(r),
                    _ => None,
                };
                serde_json::json!({
                    "role": role,
                    "name": g.map(|g| g.info.name.clone()),
                })
            }
        };
        frame("you", &data.to_string())
    }

    /// The owner's view of the LAN: the key and the guests. Never sent to
    /// a guest.
    fn admin(&self) -> String {
        let now = Instant::now();
        let key = self.key.as_ref().map(|k| {
            serde_json::json!({
                "code": display_key(&k.code),
                "state": match k.state(now) {
                    KeyState::Live => "live",
                    KeyState::Used => "used",
                    KeyState::Expired => "expired",
                    KeyState::Burned => "burned",
                },
                "expires_ms": k.expires_ms,
            })
        });
        let guests: Vec<serde_json::Value> = self
            .guests
            .iter()
            .filter(|g| matches!(g.info.status, GuestStatus::Waiting | GuestStatus::In(_)))
            .map(|g| {
                let (status, role) = match g.info.status {
                    GuestStatus::In(r) => ("in", Some(r)),
                    _ => ("waiting", None),
                };
                serde_json::json!({
                    "id": g.info.id,
                    "name": g.info.name,
                    "ip": g.info.ip.to_string(),
                    "agent": g.info.agent,
                    "status": status,
                    "role": role,
                    "online": self.online(g.info.id),
                })
            })
            .collect();
        serde_json::json!({
            "key": key,
            "guests": guests,
            "url": self.url,
            "fingerprint": self.fingerprint,
        })
        .to_string()
    }

    /// Everything a browser needs on (re)connect: it replaces what it had.
    fn snapshot(&self, who: Caller) -> String {
        let mut s = String::from("retry: 2000\n\n");
        s.push_str(&frame("hello", &self.hello));
        s.push_str(&self.you(who));
        s.push_str(&frame("state", &self.state));
        for (sid, lines) in &self.terms {
            s.push_str(&out_frame(*sid, true, lines.iter()));
        }
        if who == Caller::Owner && self.key.is_some() {
            s.push_str(&frame("admin", &self.admin()));
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
    /// LAN mode: connections that start a TLS handshake get HTTPS.
    tls: Option<Arc<rustls::ServerConfig>>,
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
    fingerprint: Option<String>,
    lan_url: Option<String>,
    hub: Arc<Mutex<Hub>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    /// Start serving. `port`: exactly that port, or `None` for DEFAULT_PORT
    /// with a fallback to any free one. `lan`: listen on every interface
    /// instead of loopback only; it needs `identity` (the HTTPS
    /// certificate), since the network only gets HTTPS.
    pub fn start(
        port: Option<u16>,
        lan: bool,
        hello: String,
        tx: Sender<AppEvent>,
        identity: Option<tls::Identity>,
    ) -> std::io::Result<Server> {
        let identity = match (lan, identity) {
            (true, None) => {
                return Err(std::io::Error::other("LAN mode needs an HTTPS certificate"))
            }
            (true, id) => id,
            (false, _) => None,
        };
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
            key: None,
            guests: Vec::new(),
            next_guest: 1,
            url: None,
            fingerprint: None,
        }));
        let fingerprint = identity.as_ref().map(|i| i.fingerprint.clone());
        let lan_url = lan
            .then(lan_ip)
            .flatten()
            .map(|ip| format!("https://{ip}:{port}"));
        {
            let mut h = lock(&hub);
            h.url = lan_url.clone();
            h.fingerprint = fingerprint.clone();
        }
        let stop = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Ctx {
            port,
            lan,
            token: token.clone(),
            hub: hub.clone(),
            tx,
            tls: identity.map(|i| i.config),
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
        let srv = Server {
            port,
            lan,
            token,
            fingerprint,
            lan_url,
            hub,
            stop,
        };
        if lan {
            srv.new_key()?;
        }
        Ok(srv)
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

    /// The address other machines open (LAN mode, when this machine's LAN
    /// address can be found). It carries no token: guests need a join key.
    pub fn lan_url(&self) -> Option<String> {
        self.lan_url.clone()
    }

    /// SHA-256 of the HTTPS certificate (LAN mode).
    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
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

    /// A corner notification (`kind`: ok / err / info), to every page, or
    /// with `owner_only` just to the owner's.
    pub fn toast(&self, kind: &str, text: &str, owner_only: bool) {
        let data = serde_json::json!({ "kind": kind, "text": text });
        let f = frame("toast", &data.to_string());
        lock(&self.hub).send_where(&f, |who| !owner_only || who == Caller::Owner);
    }

    // ---------------------------------------------------------- LAN guests

    /// Replace the join key with a new one; returns it as shown (`K7F-29Q`).
    pub fn new_key(&self) -> std::io::Result<String> {
        let code = new_key_code()?;
        let shown = display_key(&code);
        let ms = SystemTime::now()
            .checked_add(KEY_LIFE)
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        lock(&self.hub).key = Some(JoinKey {
            code,
            expires: Instant::now() + KEY_LIFE,
            expires_ms: ms,
            misses: 0,
            used: false,
        });
        Ok(shown)
    }

    /// The current join key (LAN mode).
    pub fn key(&self) -> Option<KeyInfo> {
        let hub = lock(&self.hub);
        let k = hub.key.as_ref()?;
        Some(KeyInfo {
            code: display_key(&k.code),
            state: k.state(Instant::now()),
            expires: k.expires,
        })
    }

    /// Guests waiting or in, oldest first.
    pub fn guests(&self) -> Vec<GuestInfo> {
        let hub = lock(&self.hub);
        hub.guests
            .iter()
            .filter(|g| matches!(g.info.status, GuestStatus::Waiting | GuestStatus::In(_)))
            .map(|g| GuestInfo {
                online: hub.online(g.info.id),
                ..g.info.clone()
            })
            .collect()
    }

    /// Let a waiting guest in, or change the role of one who is in. Their
    /// open pages switch at once.
    pub fn admit(&self, id: u64, role: Role) -> Option<GuestInfo> {
        let mut hub = lock(&self.hub);
        let g = hub.guest_mut(id)?;
        if !matches!(g.info.status, GuestStatus::Waiting | GuestStatus::In(_)) {
            return None;
        }
        g.info.status = GuestStatus::In(role);
        let info = g.info.clone();
        let you = hub.you(Caller::Guest(id));
        hub.send_where(&you, |who| who == Caller::Guest(id));
        Some(info)
    }

    /// Deny a waiting guest or remove one who is in: their cookie stops
    /// working and their open pages end.
    pub fn remove(&self, id: u64) -> Option<GuestInfo> {
        let mut hub = lock(&self.hub);
        let g = hub.guest_mut(id)?;
        g.info.status = match g.info.status {
            GuestStatus::Waiting => GuestStatus::Denied,
            GuestStatus::In(_) => GuestStatus::Removed,
            _ => return None,
        };
        let info = g.info.clone();
        let end = frame("end", r#"{"reason":"removed"}"#);
        hub.send_where(&end, |who| who == Caller::Guest(id));
        // dropping their senders closes those streams
        hub.clients.retain(|c| c.who != Caller::Guest(id));
        Some(info)
    }

    /// Test-only: a guest who used the key and now waits (as /api/join
    /// makes them), without going through HTTPS.
    #[cfg(test)]
    pub fn add_test_guest(&self, name: &str) -> u64 {
        let mut hub = lock(&self.hub);
        let id = hub.next_guest;
        hub.next_guest += 1;
        hub.guests.push(Guest {
            info: GuestInfo {
                id,
                name: clean_name(name),
                ip: [192, 168, 1, 23].into(),
                agent: "Chrome on Windows".into(),
                status: GuestStatus::Waiting,
                online: 0,
            },
            secret: new_token().unwrap(),
        });
        id
    }

    /// The owner's LAN data (key and guests), as the owner's page gets it.
    pub fn admin_json(&self) -> String {
        lock(&self.hub).admin()
    }

    /// Send the owner's pages a new `admin` frame.
    pub fn push_admin(&self, json: &str) {
        let f = frame("admin", json);
        lock(&self.hub).send_where(&f, |who| who == Caller::Owner);
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
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect(("192.0.2.1", 9)).ok()?; // TEST-NET-1, never actually contacted
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(b)
}

/// 128 random bits from the OS, as hex (short enough to copy from a
/// terminal without wrapping).
fn new_token() -> std::io::Result<String> {
    Ok(random_bytes::<16>()?
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect())
}

/// KEY_LEN characters from KEY_ALPHABET, without modulo bias.
fn new_key_code() -> std::io::Result<String> {
    let n = KEY_ALPHABET.len();
    let limit = 256 - 256 % n; // bytes at or above this would skew the draw
    let mut code = String::new();
    while code.len() < KEY_LEN {
        for b in random_bytes::<16>()? {
            if (b as usize) < limit && code.len() < KEY_LEN {
                code.push(KEY_ALPHABET[b as usize % n] as char);
            }
        }
    }
    Ok(code)
}

/// `K7F29Q` -> `K7F-29Q`
fn display_key(code: &str) -> String {
    let (a, b) = code.split_at(code.len() / 2);
    format!("{a}-{b}")
}

/// What a guest typed, compared the way it's shown: case, dashes and
/// spaces don't matter.
fn normalize_key(typed: &str) -> String {
    typed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A guest's name, safe to print in the terminal: letters, digits and a
/// little punctuation only (no control or direction-changing characters).
fn clean_name(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| c.is_alphanumeric() || " .-_'".contains(*c))
        .collect();
    kept.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NAME_MAX)
        .collect()
}

/// "Chrome on Windows" from a User-Agent header (never shown raw).
fn describe_agent(ua: &str) -> String {
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("OPR/") {
        "Opera"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "a browser"
    };
    let os = if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "an unknown system"
    };
    format!("{browser} on {os}")
}

/// Compare without bailing out at the first differing byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

// ------------------------------------------------------------ connections

/// One accepted connection: plain HTTP or HTTPS.
enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Conn {
    fn tcp(&self) -> &TcpStream {
        match self {
            Conn::Plain(s) => s,
            Conn::Tls(s) => &s.sock,
        }
    }

    fn close(mut self) {
        if let Conn::Tls(s) = &mut self {
            s.conn.send_close_notify();
            let _ = s.flush();
        }
        let _ = self.tcp().shutdown(Shutdown::Both);
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

/// Where a request came from.
struct Peer {
    ip: IpAddr,
    /// a loopback address: this machine
    local: bool,
    tls: bool,
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or(v6.is_loopback(), |v4| v4.is_loopback()),
        IpAddr::V4(v4) => v4.is_loopback(),
    }
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
fn read_request(conn: &mut Conn) -> std::io::Result<Request> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let mut reader = BufReader::new((&mut *conn).take(MAX_HEAD));
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
    let rest = reader.into_inner().into_inner();
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
    conn: &mut Conn,
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
    let _ = conn
        .write_all(head.as_bytes())
        .and_then(|_| conn.write_all(body))
        .and_then(|_| conn.flush());
}

/// A small self-contained page for errors (no script).
fn message_page(title: &str, text: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=en><head><meta charset=utf-8>\
         <meta name=viewport content=\"width=device-width, initial-scale=1\">\
         <title>Winsploit</title><link rel=stylesheet href=/app.css>\
         <link rel=icon href=/favicon.svg type=image/svg+xml></head>\
         <body class=message><main><div class=mark>{LOGO}<span>winsploit</span></div>\
         <h1>{title}</h1>\
         <p>{text}</p></main></body></html>"
    )
    .into_bytes()
}

fn handle(tcp: TcpStream, ctx: &Ctx) {
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(addr) = tcp.peer_addr() else { return };
    let ip = addr.ip();
    // In LAN mode the first byte decides: 0x16 starts a TLS handshake.
    let mut conn = match &ctx.tls {
        Some(cfg) => {
            let mut first = [0u8; 1];
            match tcp.peek(&mut first) {
                Ok(1) if first[0] == 0x16 => match rustls::ServerConnection::new(cfg.clone()) {
                    Ok(sc) => Conn::Tls(Box::new(rustls::StreamOwned::new(sc, tcp))),
                    Err(_) => return,
                },
                Ok(1) => Conn::Plain(tcp),
                _ => return,
            }
        }
        None => Conn::Plain(tcp),
    };
    let peer = Peer {
        ip,
        local: is_loopback(ip),
        tls: matches!(conn, Conn::Tls(_)),
    };
    let Ok(req) = read_request(&mut conn) else {
        respond(
            &mut conn,
            "400 Bad Request",
            &[],
            "text/plain",
            b"bad request",
        );
        conn.close();
        return;
    };
    let _ = conn.tcp().set_read_timeout(None);
    if let Some((code, ctype, body)) = route(&mut conn, &req, ctx, &peer) {
        respond(&mut conn, code, &[], ctype, &body);
    }
    conn.close();
}

type Reply = Option<(&'static str, &'static str, Vec<u8>)>;

/// Who a request is signed in as.
enum Access {
    Owner,
    Guest(u64, GuestStatus),
    Nobody,
}

/// The owner cookie only counts from this machine; guest cookies only
/// over HTTPS.
fn access(req: &Request, ctx: &Ctx, peer: &Peer) -> Access {
    let Some(c) = req.cookie(&ctx.cookie_name()) else {
        return Access::Nobody;
    };
    if peer.local && same(c, &ctx.token) {
        return Access::Owner;
    }
    if peer.tls {
        let hub = lock(&ctx.hub);
        if let Some(g) = hub.guests.iter().find(|g| same(c, &g.secret)) {
            return Access::Guest(g.info.id, g.info.status);
        }
    }
    Access::Nobody
}

/// Answer `req`. Returns a simple reply to send, or None when the handler
/// already wrote its own response (redirects, downloads, event streams).
fn route(conn: &mut Conn, req: &Request, ctx: &Ctx, peer: &Peer) -> Reply {
    let html = "text/html; charset=utf-8";
    let Some(host) = req
        .header("host")
        .filter(|h| host_ok(Some(h), ctx.port, ctx.lan))
    else {
        return Some((
            "421 Misdirected Request",
            "text/plain",
            b"wrong host".to_vec(),
        ));
    };
    // The network only gets HTTPS: plain HTTP from another machine (which
    // can only reach us in LAN mode) is sent to the https:// address.
    if !peer.tls && !peer.local {
        respond(
            conn,
            "308 Permanent Redirect",
            &[("Location", format!("https://{host}/"))],
            "text/plain",
            b"",
        );
        return None;
    }
    let who = access(req, ctx, peer);

    match (req.method.as_str(), req.path.as_str()) {
        // needed by the sign-in, join and error pages too
        ("GET", "/app.css") => return Some(("200 OK", "text/css; charset=utf-8", APP_CSS.into())),
        ("GET", "/favicon.svg") => return Some(("200 OK", "image/svg+xml", FAVICON.into())),
        ("GET", "/join.js") if ctx.lan => {
            return Some(("200 OK", "text/javascript; charset=utf-8", JOIN_JS.into()))
        }
        ("GET", "/") if req.query_param("t").is_some() => {
            return owner_sign_in(conn, req, ctx, peer)
        }
        ("GET", "/api/join") if ctx.lan && peer.tls => return Some(join_status(&who, ctx)),
        ("POST", "/api/join") if ctx.lan && peer.tls => return join(conn, req, ctx, peer, &who),
        _ => {}
    }

    let caller = match who {
        Access::Owner => Caller::Owner,
        Access::Guest(id, GuestStatus::In(_)) => Caller::Guest(id),
        // not let in (yet): the join page, which says where they stand
        _ if ctx.lan && peer.tls && req.method == "GET" && req.path == "/" => {
            return Some(("200 OK", html, JOIN_HTML.into()))
        }
        _ => {
            return Some((
                "401 Unauthorized",
                html,
                message_page(
                    "Open Winsploit from the terminal",
                    "The web view only accepts the link printed in your terminal. Press \
                     <b>o</b> in Winsploit to open it here.",
                ),
            ))
        }
    };
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => Some(("200 OK", html, INDEX_HTML.into())),
        ("GET", "/app.js") => Some(("200 OK", "text/javascript; charset=utf-8", APP_JS.into())),
        ("GET", "/events") => {
            serve_events(conn, ctx, caller);
            None
        }
        ("GET", "/api/log") => serve_log(conn, req, ctx),
        ("POST", "/api/action") => Some(post_action(req, ctx, peer, caller)),
        _ => Some(("404 Not Found", "text/plain", b"not found".to_vec())),
    }
}

/// `/?t=<token>`: the owner's sign-in. Only from this machine, so the
/// token never has to travel over the network.
fn owner_sign_in(conn: &mut Conn, req: &Request, ctx: &Ctx, peer: &Peer) -> Reply {
    let html = "text/html; charset=utf-8";
    if !peer.local {
        return Some((
            "403 Forbidden",
            html,
            message_page(
                "This link only works on the host",
                "The sign-in link is for the computer running Winsploit. From another \
                 device, open the address without it and enter a join key.",
            ),
        ));
    }
    if !req.query_param("t").is_some_and(|t| same(t, &ctx.token)) {
        return Some((
            "403 Forbidden",
            html,
            message_page(
                "This link has expired",
                "Each Winsploit session has its own link. Press <b>o</b> in the \
                 terminal to open the current one.",
            ),
        ));
    }
    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/",
        ctx.cookie_name(),
        ctx.token
    );
    respond(
        conn,
        "303 See Other",
        &[("Location", "/".into()), ("Set-Cookie", cookie)],
        "text/plain",
        b"",
    );
    None
}

fn json_reply(status: &'static str, v: serde_json::Value) -> (&'static str, &'static str, Vec<u8>) {
    (status, "application/json", v.to_string().into_bytes())
}

fn status_json(status: GuestStatus) -> serde_json::Value {
    match status {
        GuestStatus::Waiting => serde_json::json!({ "status": "waiting" }),
        GuestStatus::In(r) => serde_json::json!({ "status": "in", "role": r }),
        GuestStatus::Denied => serde_json::json!({ "status": "denied" }),
        GuestStatus::Removed => serde_json::json!({ "status": "removed" }),
    }
}

/// `GET /api/join`: where this browser stands (the join page polls it).
fn join_status(who: &Access, ctx: &Ctx) -> (&'static str, &'static str, Vec<u8>) {
    let v = match who {
        Access::Guest(id, _) => {
            // read it again: it may have changed since the cookie check
            let hub = lock(&ctx.hub);
            hub.guest(*id)
                .map_or(serde_json::json!({ "status": "none" }), |g| {
                    status_json(g.info.status)
                })
        }
        Access::Owner => serde_json::json!({ "status": "owner" }),
        Access::Nobody => serde_json::json!({ "status": "none" }),
    };
    json_reply("200 OK", v)
}

#[derive(Deserialize)]
struct JoinBody {
    key: String,
    #[serde(default)]
    name: String,
}

/// `POST /api/join {key, name}`: a valid, unused, unexpired key makes this
/// browser a waiting guest (with its own cookie) and asks the terminal.
fn join(conn: &mut Conn, req: &Request, ctx: &Ctx, peer: &Peer, who: &Access) -> Reply {
    if !origin_ok(req, ctx.port, ctx.lan, peer.tls) {
        return Some((
            "403 Forbidden",
            "text/plain",
            b"cross-origin request".to_vec(),
        ));
    }
    if let Access::Guest(_, status @ (GuestStatus::Waiting | GuestStatus::In(_))) = who {
        return Some(json_reply("200 OK", status_json(*status)));
    }
    let Ok(body) = serde_json::from_slice::<JoinBody>(&req.body) else {
        return Some((
            "400 Bad Request",
            "text/plain",
            b"bad join request".to_vec(),
        ));
    };
    let typed = normalize_key(&body.key);
    let secret = match new_token() {
        Ok(s) => s,
        Err(_) => return Some(("500 Internal Server Error", "text/plain", Vec::new())),
    };
    let mut burned = false;
    let outcome = {
        let mut hub = lock(&ctx.hub);
        let now = Instant::now();
        match hub.key.as_mut() {
            Some(k) if k.state(now) == KeyState::Live => {
                if same(&typed, &k.code) {
                    k.used = true;
                    let id = hub.next_guest;
                    hub.next_guest += 1;
                    hub.guests.push(Guest {
                        info: GuestInfo {
                            id,
                            name: clean_name(&body.name),
                            ip: peer.ip,
                            agent: describe_agent(req.header("user-agent").unwrap_or("")),
                            status: GuestStatus::Waiting,
                            online: 0,
                        },
                        secret: secret.clone(),
                    });
                    Ok(id)
                } else {
                    k.misses += 1;
                    burned = k.misses >= KEY_TRIES;
                    Err(serde_json::json!({
                        "error": "wrong",
                        "left": KEY_TRIES.saturating_sub(k.misses),
                    }))
                }
            }
            _ => Err(serde_json::json!({ "error": "no_key" })),
        }
    };
    match outcome {
        Ok(id) => {
            let _ = ctx.tx.send(AppEvent::Lan(LanEvent::JoinRequest(id)));
            let cookie = format!(
                "{}={secret}; HttpOnly; Secure; SameSite=Strict; Path=/",
                ctx.cookie_name()
            );
            respond(
                conn,
                "200 OK",
                &[("Set-Cookie", cookie)],
                "application/json",
                status_json(GuestStatus::Waiting).to_string().as_bytes(),
            );
            None
        }
        Err(v) => {
            if burned {
                let _ = ctx.tx.send(AppEvent::Lan(LanEvent::KeyBurned));
            }
            std::thread::sleep(WRONG_KEY_DELAY);
            Some(json_reply("403 Forbidden", v))
        }
    }
}

/// The Iso cube in the fixed Frost colours (design/README.md): a tab icon
/// can't read the page's CSS variables.
const FAVICON: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'>\
<path d='M16 2.5 28 9.3v13.4L16 29.5 4 22.7V9.3z' fill='#46A0B9'/>\
<path d='M16 2.5 28 9.3 16 16 4 9.3z' fill='#BCDDE6'/>\
<path d='M16 16 28 9.3v13.4L16 29.5z' fill='#89C2D2'/>\
<path d='M10.5 8l3 1.6-3 1.7M15 11.5l3.2 1.8' fill='none' stroke='#0C191D' \
stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round'/></svg>";

/// The Iso cube for the message pages, coloured by app.css's `.logo` rules.
const LOGO: &str = "<svg class=logo viewBox='0 0 32 32' aria-hidden=true>\
<path class=d d='M16 2.5 28 9.3v13.4L16 29.5 4 22.7V9.3z'/>\
<path class=t d='M16 2.5 28 9.3 16 16 4 9.3z'/>\
<path class=r d='M16 16 28 9.3v13.4L16 29.5z'/>\
<path class=k d='M10.5 8l3 1.6-3 1.7M15 11.5l3.2 1.8'/></svg>";

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
/// the (valid) Host it was sent to, with the scheme it was sent over, and a
/// JSON body (a cross-site form can't send that without a CORS preflight,
/// which is never answered).
fn origin_ok(req: &Request, port: u16, lan: bool, tls: bool) -> bool {
    let scheme = if tls { "https" } else { "http" };
    let host = req.header("host");
    let origin = host.is_some_and(|h| {
        host_ok(Some(h), port, lan) && req.header("origin") == Some(&format!("{scheme}://{h}"))
    });
    let json = req
        .header("content-type")
        .is_some_and(|c| c.trim_start().starts_with("application/json"));
    let fetch_site = req
        .header("sec-fetch-site")
        .is_none_or(|s| s == "same-origin");
    origin && json && fetch_site
}

fn post_action(
    req: &Request,
    ctx: &Ctx,
    peer: &Peer,
    caller: Caller,
) -> (&'static str, &'static str, Vec<u8>) {
    if !origin_ok(req, ctx.port, ctx.lan, peer.tls) {
        return (
            "403 Forbidden",
            "text/plain",
            b"cross-origin request".to_vec(),
        );
    }
    let action = match serde_json::from_slice::<WebAction>(&req.body) {
        Ok(a) => a,
        Err(e) => {
            return (
                "400 Bad Request",
                "text/plain",
                format!("bad action: {e}").into_bytes(),
            )
        }
    };
    // a guest's role is read now, so a change applies to the next click
    if let Caller::Guest(id) = caller {
        let status = lock(&ctx.hub).guest(id).map(|g| g.info.status);
        match status {
            Some(GuestStatus::In(Role::Control)) if !action.owner_only() => {}
            Some(GuestStatus::In(Role::Control)) => {
                return (
                    "403 Forbidden",
                    "text/plain",
                    b"only the host manages guests".to_vec(),
                )
            }
            Some(GuestStatus::In(Role::Watch)) => {
                return ("403 Forbidden", "text/plain", b"watch only".to_vec())
            }
            _ => return ("401 Unauthorized", "text/plain", b"not let in".to_vec()),
        }
    }
    if ctx.tx.send(AppEvent::Web(action, caller)).is_err() {
        return (
            "503 Service Unavailable",
            "text/plain",
            b"session ended".to_vec(),
        );
    }
    ("202 Accepted", "application/json", b"{}".to_vec())
}

/// A terminal's run log, as a download. Only paths the app registered.
fn serve_log(conn: &mut Conn, req: &Request, ctx: &Ctx) -> Reply {
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
        conn,
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

fn serve_events(conn: &mut Conn, ctx: &Ctx, who: Caller) {
    let (tx, rx) = mpsc::channel::<String>();
    let snapshot = {
        let mut hub = lock(&ctx.hub);
        if !hub.ended {
            hub.clients.push(Client { who, tx });
        }
        hub.snapshot(who)
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
         Connection: close\r\n{SECURITY_HEADERS}\r\n"
    );
    let mut send = |s: &str| conn.write_all(s.as_bytes()).and_then(|_| conn.flush());
    if send(&head).and_then(|_| send(&snapshot)).is_err() {
        return;
    }
    // Runs until the tab goes away (a write fails) or its sender is dropped:
    // the server stopping, or this guest being removed. Both queue an `end`
    // frame first, and the frames still queued are written before
    // Disconnected.
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
        assert!(origin_ok(&req(&good), 7878, false, false));
        let with = |k: &str, v: &str| {
            let mut h: Vec<(&str, &str)> = good.iter().filter(|(n, _)| *n != k).cloned().collect();
            h.push((k, v));
            req(&h.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>())
        };
        assert!(!origin_ok(
            &with("Origin", "http://evil.example"),
            7878,
            false,
            false
        ));
        assert!(!origin_ok(
            &with("Content-Type", "text/plain"),
            7878,
            false,
            false
        ));
        assert!(!origin_ok(
            &with("Sec-Fetch-Site", "cross-site"),
            7878,
            false,
            false
        ));
        // the Origin must match the Host the request was sent to
        assert!(!origin_ok(
            &with("Origin", "http://localhost:7878"),
            7878,
            false,
            false
        ));
        // on the LAN: our LAN address, in LAN mode only, over https
        let lan = [
            ("Host", "192.168.1.23:7878"),
            ("Origin", "https://192.168.1.23:7878"),
            ("Content-Type", "application/json"),
        ];
        assert!(origin_ok(&req(&lan), 7878, true, true));
        assert!(!origin_ok(&req(&lan), 7878, false, true));
        // the scheme counts: an http:// page can't post to the https one
        assert!(!origin_ok(&req(&lan), 7878, true, false));
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
        let admit: WebAction =
            serde_json::from_str(r#"{"type":"admit","guest":3,"role":"watch"}"#).unwrap();
        assert!(admit.owner_only());
        assert_eq!(
            serde_json::from_str::<WebAction>(r#"{"type":"newkey"}"#).unwrap(),
            WebAction::Newkey
        );
        assert!(serde_json::from_str::<WebAction>(r#"{"type":"shell","cmd":"x"}"#).is_err());
        assert!(
            serde_json::from_str::<WebAction>(r#"{"type":"admit","guest":3,"role":"root"}"#)
                .is_err()
        );
    }

    #[test]
    fn keys_names_and_agents() {
        let k = new_key_code().unwrap();
        assert_eq!(k.len(), KEY_LEN);
        assert!(k.bytes().all(|b| KEY_ALPHABET.contains(&b)));
        assert_ne!(k, new_key_code().unwrap());
        assert_eq!(display_key("K7F29Q"), "K7F-29Q");
        assert_eq!(normalize_key(" k7f-29q "), "K7F29Q");
        // nothing that could move the cursor or fake a line in the terminal
        assert_eq!(
            clean_name("Taha\u{1b}[2J\u{202e}  ben\nissa"),
            "Taha2J ben issa"
        );
        assert_eq!(clean_name(&"x".repeat(100)).len(), NAME_MAX);
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
        assert_eq!(describe_agent(ua), "Chrome on Windows");
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
                      AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
        assert_eq!(describe_agent(iphone), "Safari on iOS");
        assert!(is_loopback("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback("192.168.1.5".parse().unwrap()));
    }

    #[test]
    fn snapshot_has_every_terminal_and_admin_only_for_the_owner() {
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
            key: Some(JoinKey {
                code: "K7F29Q".into(),
                expires: Instant::now() + KEY_LIFE,
                expires_ms: 0,
                misses: 0,
                used: false,
            }),
            guests: vec![],
            next_guest: 1,
            url: None,
            fingerprint: None,
        };
        let s = h.snapshot(Caller::Owner);
        assert!(s.contains(r#""sid":0"#) && s.contains("console line"));
        assert!(s.contains(r#""sid":3"#) && s.contains("nmap line"));
        assert!(s.contains(r#""reset":true"#));
        assert!(s.contains("event: admin") && s.contains("K7F-29Q"));
        assert!(s.contains(r#""role":"owner""#));
        let g = h.snapshot(Caller::Guest(1));
        assert!(g.contains("nmap line"));
        assert!(!g.contains("event: admin") && !g.contains("K7F"));
    }

    // ------------------------------------------------- over real sockets

    fn raw(port: u16, text: String) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(text.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }

    /// End to end over a real socket: sign in, stream, act, download, and
    /// the guards.
    #[test]
    fn serves_signs_in_streams_and_guards() {
        let (tx, rx) = mpsc::channel();
        let srv = Server::start(Some(0), false, r#"{"hi":1}"#.into(), tx, None).unwrap();
        srv.push_lines(0, true, vec!["first".into()]);
        let port = srv.port();
        let host = format!("127.0.0.1:{port}");
        let raw = |text: String| raw(port, text);
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
        // no LAN, no join
        assert!(get("/api/join", "").starts_with("HTTP/1.1 401"));
        assert!(srv.key().is_none());

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
            Ok(AppEvent::Web(WebAction::Cancel { term: 1 }, Caller::Owner)) => {}
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
        srv.toast("ok", "nmap finished", false);
        read_until("nmap finished", &mut seen);
        drop(srv);
        read_until("event: end", &mut seen);
    }

    /// Test-only: accept the server's certificate if its SHA-256 is the
    /// one the terminal shows (what a careful guest checks by eye).
    #[derive(Debug)]
    struct Pinned(String);

    impl rustls::client::danger::ServerCertVerifier for Pinned {
        fn verify_server_cert(
            &self,
            cert: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: &rustls::pki_types::ServerName<'_>,
            _: &[u8],
            _: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            let fp = ring::digest::digest(&ring::digest::SHA256, cert)
                .as_ref()
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(":");
            if fp == self.0 {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("fingerprint mismatch".into()))
            }
        }

        fn verify_tls12_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            let algs = rustls::crypto::ring::default_provider().signature_verification_algorithms;
            rustls::crypto::verify_tls12_signature(m, c, d, &algs)
        }

        fn verify_tls13_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            let algs = rustls::crypto::ring::default_provider().signature_verification_algorithms;
            rustls::crypto::verify_tls13_signature(m, c, d, &algs)
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    fn tls_client(fp: &str) -> Arc<rustls::ClientConfig> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(Pinned(fp.to_string())))
                .with_no_client_auth(),
        )
    }

    type TlsStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

    fn tls_connect(cfg: &Arc<rustls::ClientConfig>, ip: IpAddr, port: u16) -> TlsStream {
        let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let conn = rustls::ClientConnection::new(cfg.clone(), name).unwrap();
        let tcp = TcpStream::connect((ip, port)).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        rustls::StreamOwned::new(conn, tcp)
    }

    /// One request over HTTPS; everything the server sent back.
    fn https(cfg: &Arc<rustls::ClientConfig>, ip: IpAddr, port: u16, text: &str) -> String {
        let mut s = tls_connect(cfg, ip, port);
        s.write_all(text.as_bytes()).unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn set_cookie(resp: &str) -> String {
        resp.lines()
            .find_map(|l| l.strip_prefix("Set-Cookie: "))
            .and_then(|c| c.split(';').next())
            .expect("no cookie set")
            .to_string()
    }

    /// The whole guest journey over HTTPS: wrong key, right key (once),
    /// waiting, let in as a watcher, upgraded, removed.
    #[test]
    fn lan_guests_join_with_a_one_time_key_and_approval() {
        let (tx, rx) = mpsc::channel();
        let id = tls::load_or_create(None).unwrap();
        let fp = id.fingerprint.clone();
        let srv = Server::start(Some(0), true, "{}".into(), tx, Some(id)).unwrap();
        assert_eq!(srv.fingerprint(), Some(fp.as_str()));
        if let Some(url) = srv.lan_url() {
            assert!(url.starts_with("https://") && !url.contains("t="));
        }
        let port = srv.port();
        let lo: IpAddr = [127, 0, 0, 1].into();
        let cfg = tls_client(&fp);
        let host = format!("127.0.0.1:{port}");
        let https = |text: String| https(&cfg, lo, port, &text);
        let post_join = |body: &str, cookie: &str| {
            https(format!(
                "POST /api/join HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\n\
                 Content-Type: application/json\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0) \
                 Chrome/130.0 Safari/537.36\r\n{cookie}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ))
        };
        let get = |path: &str, cookie: &str| {
            https(format!(
                "GET {path} HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\n"
            ))
        };

        // the local owner link still works over plain HTTP
        assert!(raw(
            port,
            format!("GET /?t={} HTTP/1.1\r\nHost: {host}\r\n\r\n", srv.token)
        )
        .starts_with("HTTP/1.1 303"));

        // a stranger gets the join page, not the dashboard
        let page = get("/", "");
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("join.js"));
        assert!(get("/events", "").starts_with("HTTP/1.1 401"));

        // a wrong key is refused and counted
        let key = srv.key().unwrap();
        assert_eq!(key.state, KeyState::Live);
        let wrong = post_join(r#"{"key":"AAA-AAA"}"#, "");
        assert!(wrong.starts_with("HTTP/1.1 403") && wrong.contains(r#""left":4"#));

        // the right key (any case, any dashes) makes a waiting guest
        let typed = key.code.to_lowercase().replace('-', " ");
        let ok = post_join(&format!(r#"{{"key":"{typed}","name":"Taha\u001b"}}"#), "");
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains("Secure") && ok.contains("HttpOnly"));
        let cookie = format!("Cookie: {}\r\n", set_cookie(&ok));
        let gid = match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(AppEvent::Lan(LanEvent::JoinRequest(id))) => id,
            _ => panic!("no join request"),
        };
        let g = &srv.guests()[0];
        assert_eq!(
            (g.id, g.name.as_str(), g.agent.as_str()),
            (gid, "Taha", "Chrome on Windows")
        );
        assert_eq!(g.status, GuestStatus::Waiting);
        // ...and only once
        assert_eq!(srv.key().unwrap().state, KeyState::Used);
        assert!(post_join(&format!(r#"{{"key":"{}"}}"#, key.code), "").contains("no_key"));

        // waiting: still the join page, no stream, no actions
        assert!(get("/api/join", &cookie).contains(r#""status":"waiting""#));
        assert!(get("/", &cookie).contains("join.js"));
        assert!(get("/events", &cookie).starts_with("HTTP/1.1 401"));

        // let in as a watcher: the dashboard, the stream, but no actions
        srv.admit(gid, Role::Watch).unwrap();
        assert!(get("/api/join", &cookie).contains(r#""role":"watch""#));
        assert!(get("/", &cookie).contains("app.js"));
        let act = |body: &str| {
            https(format!(
                "POST /api/action HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\n\
                 {cookie}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ))
        };
        assert!(act(r#"{"type":"redetect"}"#).starts_with("HTTP/1.1 403"));

        // full control: actions arrive, tagged with who sent them
        srv.admit(gid, Role::Control).unwrap();
        assert!(act(r#"{"type":"redetect"}"#).starts_with("HTTP/1.1 202"));
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(AppEvent::Web(WebAction::Redetect, Caller::Guest(id))) => assert_eq!(id, gid),
            _ => panic!("guest action didn't arrive"),
        }
        // but guests never manage guests
        assert!(act(r#"{"type":"newkey"}"#).starts_with("HTTP/1.1 403"));
        let admit_self = format!(r#"{{"type":"admit","guest":{gid},"role":"control"}}"#);
        assert!(act(&admit_self).starts_with("HTTP/1.1 403"));

        // a guest's stream: their role, no admin data; removal ends it
        let mut es = tls_connect(&cfg, lo, port);
        write!(es, "GET /events HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\n").unwrap();
        let mut seen = String::new();
        let mut buf = [0u8; 4096];
        let mut read_until = |needle: &str, seen: &mut String| {
            while !seen.contains(needle) {
                let n = es.read(&mut buf).expect("stream stalled");
                assert!(n > 0, "stream closed before {needle:?}");
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        };
        read_until("event: state", &mut seen);
        assert!(seen.contains(r#""role":"control""#));
        assert!(!seen.contains("event: admin"));
        srv.toast("info", "owner secret", true);
        srv.toast("info", "for everyone", false);
        read_until("for everyone", &mut seen);
        assert!(!seen.contains("owner secret"));
        srv.admit(gid, Role::Watch).unwrap();
        read_until(r#""role":"watch""#, &mut seen);
        srv.remove(gid).unwrap();
        read_until(r#""reason":"removed""#, &mut seen);
        assert!(srv.guests().is_empty());
        assert!(get("/api/join", &cookie).contains(r#""status":"removed""#));
        assert!(get("/events", &cookie).starts_with("HTTP/1.1 401"));
    }

    #[test]
    fn wrong_keys_burn_the_key_and_expired_keys_fail() {
        let (tx, rx) = mpsc::channel();
        let id = tls::load_or_create(None).unwrap();
        let cfg = tls_client(&id.fingerprint);
        let srv = Server::start(Some(0), true, "{}".into(), tx, Some(id)).unwrap();
        let port = srv.port();
        let lo: IpAddr = [127, 0, 0, 1].into();
        let host = format!("127.0.0.1:{port}");
        let post_join = |key: &str| {
            let body = format!(r#"{{"key":"{key}"}}"#);
            https(
                &cfg,
                lo,
                port,
                &format!(
                    "POST /api/join HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                ),
            )
        };
        let right = srv.key().unwrap().code;
        for _ in 0..KEY_TRIES {
            assert!(post_join("ZZZ-ZZZ").starts_with("HTTP/1.1 403"));
        }
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)),
            Ok(AppEvent::Lan(LanEvent::KeyBurned))
        ));
        assert_eq!(srv.key().unwrap().state, KeyState::Burned);
        // even the right key is no good now
        assert!(post_join(&right).contains("no_key"));
        // a new key works; an expired one doesn't
        let fresh = srv.new_key().unwrap();
        lock(&srv.hub).key.as_mut().unwrap().expires = Instant::now();
        assert!(post_join(&fresh).contains("no_key"));
        // a cross-site form can't join either
        let forged = https(
            &cfg,
            lo,
            port,
            &format!(
                "POST /api/join HTTP/1.1\r\nHost: {host}\r\nOrigin: https://evil.example\r\n\
                 Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
            ),
        );
        assert!(forged.starts_with("HTTP/1.1 403"));
    }

    /// From another machine (this machine's own LAN address stands in for
    /// one): plain HTTP is redirected to HTTPS, and the owner link refused.
    #[test]
    fn the_network_only_gets_https_and_never_the_owner_token() {
        let Some(ip) = lan_ip() else { return }; // no network: nothing to test
        let (tx, _rx) = mpsc::channel();
        let id = tls::load_or_create(None).unwrap();
        let cfg = tls_client(&id.fingerprint);
        let srv = Server::start(Some(0), true, "{}".into(), tx, Some(id)).unwrap();
        let port = srv.port();
        let host = format!("{ip}:{port}");
        // reachable on loopback too (0.0.0.0 covers it)
        assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());

        let mut s = TcpStream::connect((ip, port)).unwrap();
        write!(s, "GET /?t={} HTTP/1.1\r\nHost: {host}\r\n\r\n", srv.token).unwrap();
        let mut plain = String::new();
        let _ = s.read_to_string(&mut plain);
        assert!(plain.starts_with("HTTP/1.1 308"), "{plain}");
        assert!(plain.contains(&format!("Location: https://{host}/")));

        let r = https(
            &cfg,
            ip,
            port,
            &format!("GET /?t={} HTTP/1.1\r\nHost: {host}\r\n\r\n", srv.token),
        );
        assert!(r.starts_with("HTTP/1.1 403") && !r.contains("Set-Cookie"));
        // nor does a stolen owner cookie work from there
        let r = https(
            &cfg,
            ip,
            port,
            &format!(
                "GET /events HTTP/1.1\r\nHost: {host}\r\nCookie: winsploit_{port}={}\r\n\r\n",
                srv.token
            ),
        );
        assert!(r.starts_with("HTTP/1.1 401"));
    }
}
