//! Connection to a Flydigi B9X.
//!
//! A backend (BlueZ on Linux, or the fake device) turns a GATT connection into a
//! [`Link`]: a channel of writes in and notifications out. [`Device`] sits on top of
//! any link. It keeps one connection open, routes the 1 Hz status push to a watch
//! channel and replies to whoever is waiting, and only writes [`Frame`]s, so every
//! byte passes the whitelist in `b9x-proto`. [`control`] has the multi-step changes
//! the app makes, shared by the CLI and the GUI.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use b9x_proto::{self as p, Frame, LedSettings, ShutdownInfo, Status, UnsafeFrame};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::time::{Instant, timeout_at};

#[cfg(target_os = "linux")]
pub mod bluez;
pub mod control;
pub mod fake;

// Each OS gets its own backend module with `scan`, `find` and `connect`.
#[cfg(target_os = "linux")]
pub use bluez::{connect, disconnect, find, find_all, not_found, scan};
#[cfg(not(target_os = "linux"))]
pub use unsupported::{connect, disconnect, find, find_all, not_found, scan};

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use super::*;

    fn no_backend<T>() -> Result<T> {
        Err(Error::Ble("no BLE backend for this OS yet".into()))
    }

    pub async fn scan(_timeout: Duration) -> Result<Vec<Found>> {
        no_backend()
    }

    pub async fn find(_timeout: Duration) -> Result<String> {
        no_backend()
    }

    pub async fn find_all(_timeout: Duration, _settle: Duration) -> Result<Vec<Found>> {
        no_backend()
    }

    pub fn not_found() -> Error {
        Error::NotFound("no B9X found".into())
    }

    pub async fn connect(_address: &str, _mode: WriteMode, _scan_timeout: Duration) -> Result<Link> {
        no_backend()
    }

    pub async fn disconnect(_address: Option<&str>) -> Result<Vec<String>> {
        no_backend()
    }
}

pub const NAME_HINT: &str = "B9X";
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a written mode or quick switch overrides status pushes that have not
/// caught up yet.
pub const WRITE_HINT_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Unsafe(#[from] UnsafeFrame),
    #[error("{0}")]
    Invalid(String),
    #[error("no reply from the device")]
    NoReply,
    #[error("disconnected: {0}")]
    Disconnected(String),
    #[error("{0}")]
    NotFound(String),
    /// Connected, but the device did not identify itself as the one we asked for.
    #[error("{0}")]
    NotB9x(String),
    #[error("bluetooth: {0}")]
    Ble(String),
}

impl From<p::ValueError> for Error {
    fn from(e: p::ValueError) -> Self {
        Error::Invalid(e.0)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How writes go out. The app uses write without response, but the characteristic
/// does not advertise it, so BlueZ and macOS may refuse it; request is the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WriteMode {
    #[default]
    Request,
    Command,
}

// --- link --------------------------------------------------------------------

pub enum LinkRequest {
    Write(Frame, oneshot::Sender<Result<()>>),
    Close(oneshot::Sender<()>),
}

pub enum LinkEvent {
    Notify(Vec<u8>),
    Disconnected(String),
}

/// What a backend hands over once the notify subscription is active.
pub struct Link {
    pub address: String,
    pub requests: mpsc::Sender<LinkRequest>,
    pub events: mpsc::UnboundedReceiver<LinkEvent>,
}

/// One device seen while scanning.
#[derive(Debug, Clone)]
pub struct Found {
    pub address: String,
    pub name: String,
    pub rssi: Option<i16>,
}

// --- device ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Connected,
    Disconnected(String),
}

pub struct Device {
    address: String,
    debug: bool,
    requests: mpsc::Sender<LinkRequest>,
    /// Held for the whole request so replies cannot interleave.
    replies: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    status: watch::Receiver<Option<Status>>,
    state: watch::Receiver<LinkState>,
    version: Arc<StdMutex<Option<String>>>,
    written: StdMutex<Written>,
    identity_warning: Option<String>,
}

/// Mode and quick switch we wrote last. Status pushes lag behind writes by a second
/// or more, so decisions made right after a write go by these instead.
#[derive(Default)]
struct Written {
    mode: Option<(u8, Instant)>,
    quick_switch: Option<(u8, Instant)>,
}

fn hint(slot: &mut Option<(u8, Instant)>, reported: u8) -> u8 {
    match *slot {
        Some((v, _)) if v == reported => {
            *slot = None;
            v
        }
        Some((v, at)) if at.elapsed() < WRITE_HINT_TTL => v,
        _ => {
            *slot = None;
            reported
        }
    }
}

impl Device {
    /// Take over a link and do the app's handshake.
    pub async fn open(link: Link, debug: bool) -> Result<Device> {
        let (reply_tx, reply_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(None);
        let (state_tx, state_rx) = watch::channel(LinkState::Connected);
        let version = Arc::new(StdMutex::new(None));
        tokio::spawn(dispatch(link.events, debug, reply_tx, status_tx, state_tx, version.clone()));
        let mut dev = Device {
            address: link.address,
            debug,
            requests: link.requests,
            replies: Mutex::new(reply_rx),
            status: status_rx,
            state: state_rx,
            version,
            written: StdMutex::default(),
            identity_warning: None,
        };
        // The device pushes its firmware version as ASCII right after subscribing.
        let reply = dev.request(&p::HANDSHAKE, |f| f.starts_with(&p::HANDSHAKE[..2])).await?;
        // Nothing else is written until the device has identified itself.
        if let Err(e) = dev.check_identity(&reply) {
            dev.disconnect().await;
            return Err(e);
        }
        Ok(dev)
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// Set when the device answered the handshake with an unexpected product id.
    pub fn identity_warning(&self) -> Option<&str> {
        self.identity_warning.as_deref()
    }

    /// The reply carries the device's own MAC; it must be the address we connected to.
    /// The product id has only been seen on one unit, so a different one is a warning.
    fn check_identity(&mut self, reply: &[u8]) -> Result<()> {
        let hs = p::parse_handshake(reply).map_err(|e| Error::NotB9x(e.0))?;
        // macOS gives a CoreBluetooth UUID instead of the MAC; nothing to compare there.
        let is_mac = self.address.len() == 17 && self.address.bytes().filter(|&b| b == b':').count() == 5;
        if is_mac && !hs.mac.eq_ignore_ascii_case(&self.address) {
            return Err(Error::NotB9x(format!(
                "{} answered the handshake as {}; not the B9X we connected to",
                self.address, hs.mac
            )));
        }
        if hs.product_id != p::B9X_PRODUCT_ID {
            self.identity_warning = Some(format!(
                "unexpected product id {} in the handshake (B9X: {}); this may not be a B9X",
                p::hex(&hs.product_id),
                p::hex(&p::B9X_PRODUCT_ID)
            ));
        }
        Ok(())
    }

    pub fn version(&self) -> Option<String> {
        self.version.lock().unwrap().clone()
    }

    /// Latest status push, updated about once a second.
    pub fn watch_status(&self) -> watch::Receiver<Option<Status>> {
        self.status.clone()
    }

    pub fn watch_state(&self) -> watch::Receiver<LinkState> {
        self.state.clone()
    }

    fn check_connected(&self) -> Result<()> {
        match &*self.state.borrow() {
            LinkState::Connected => Ok(()),
            LinkState::Disconnected(why) => Err(Error::Disconnected(why.clone())),
        }
    }

    pub async fn write(&self, bytes: &[u8]) -> Result<()> {
        let frame = Frame::check(bytes)?;
        self.check_connected()?;
        if self.debug {
            eprintln!("-> {}", p::hex(frame.as_bytes()));
        }
        let (tx, rx) = oneshot::channel();
        let gone = || Error::Disconnected("link closed".into());
        self.requests.send(LinkRequest::Write(frame, tx)).await.map_err(|_| gone())?;
        rx.await.map_err(|_| gone())??;
        let mut w = self.written.lock().unwrap();
        match bytes {
            [0x20, mode, ..] => w.mode = Some((*mode, Instant::now())),
            [0x21, mode] => w.quick_switch = Some((*mode, Instant::now())),
            _ => {}
        }
        Ok(())
    }

    async fn expect(
        replies: &mut mpsc::UnboundedReceiver<Vec<u8>>,
        deadline: Instant,
        mut matches: impl FnMut(&[u8]) -> bool,
    ) -> Result<Vec<u8>> {
        loop {
            match timeout_at(deadline, replies.recv()).await {
                Err(_) => return Err(Error::NoReply),
                Ok(None) => return Err(Error::Disconnected("link closed".into())),
                Ok(Some(f)) if matches(&f) => return Ok(f),
                Ok(Some(_)) => {}
            }
        }
    }

    async fn request(&self, frame: &[u8], matches: impl FnMut(&[u8]) -> bool) -> Result<Vec<u8>> {
        let mut replies = self.replies.lock().await;
        while replies.try_recv().is_ok() {}
        self.write(frame).await?;
        Self::expect(&mut replies, Instant::now() + REPLY_TIMEOUT, matches).await
    }

    /// Wait for the next status push.
    pub async fn status(&self) -> Result<Status> {
        let mut rx = self.status.clone();
        rx.mark_unchanged();
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            match timeout_at(deadline, rx.changed()).await {
                Err(_) => return Err(Error::NoReply),
                Ok(Err(_)) => return Err(Error::Disconnected("link closed".into())),
                Ok(Ok(())) => {}
            }
            if let Some(s) = rx.borrow_and_update().clone() {
                return Ok(s);
            }
        }
    }

    /// Next status push, with the mode and quick switch we wrote in the last few
    /// seconds in place of values the device has not reported yet.
    pub async fn status_now(&self) -> Result<Status> {
        let mut s = self.status().await?;
        let mut w = self.written.lock().unwrap();
        s.mode = hint(&mut w.mode, s.mode);
        s.quick_switch = hint(&mut w.quick_switch, s.quick_switch);
        Ok(s)
    }

    pub async fn curve(&self) -> Result<Vec<u8>> {
        Ok(p::parse_curve(&self.request(&p::QUERY_CURVE, |f| f.starts_with(&p::QUERY_CURVE)).await?)?)
    }

    pub async fn mac(&self) -> Result<String> {
        Ok(p::parse_mac(&self.request(&p::QUERY_MAC, |f| f.first() == Some(&0x24)).await?)?)
    }

    pub async fn shutdown(&self) -> Result<ShutdownInfo> {
        Ok(p::parse_shutdown(&self.request(&p::QUERY_SHUTDOWN, |f| f.first() == Some(&0x61)).await?)?)
    }

    /// LED settings of `code`, or of the current effect for 0.
    pub async fn led(&self, code: u8) -> Result<LedSettings> {
        let matches = move |f: &[u8]| f.len() >= 20 && f[0] == 0x51 && (code == 0 || f[1] == code);
        let mut replies = self.replies.lock().await;
        while replies.try_recv().is_ok() {}
        self.write(&p::led_query(code)).await?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let first = Self::expect(&mut replies, deadline, matches).await?;
        let mut frames = vec![first.clone()];
        if p::led_frames_needed(&first) > 1 {
            let second = Self::expect(&mut replies, deadline, |f| matches(f) && f[1] == first[1] && f[4] == 2).await?;
            frames.push(second);
        }
        Ok(p::parse_led_reply(&frames))
    }

    pub async fn disconnect(&self) {
        let (tx, rx) = oneshot::channel();
        if self.requests.send(LinkRequest::Close(tx)).await.is_ok() {
            let _ = rx.await;
        }
    }
}

async fn dispatch(
    mut events: mpsc::UnboundedReceiver<LinkEvent>,
    debug: bool,
    replies: mpsc::UnboundedSender<Vec<u8>>,
    status: watch::Sender<Option<Status>>,
    state: watch::Sender<LinkState>,
    version: Arc<StdMutex<Option<String>>>,
) {
    let why = loop {
        match events.recv().await {
            Some(LinkEvent::Notify(data)) => {
                if debug {
                    eprintln!("<- {}", p::hex(&data));
                }
                if data.first().is_some_and(u8::is_ascii_digit) {
                    let mut v = version.lock().unwrap();
                    if v.is_none() {
                        let text = data.split(|&b| b == 0).next().unwrap_or_default();
                        *v = Some(String::from_utf8_lossy(text).into_owned());
                    }
                }
                if data.first() == Some(&0x13) {
                    if let Ok(s) = p::parse_status(&data) {
                        status.send_replace(Some(s));
                    }
                } else {
                    let _ = replies.send(data);
                }
            }
            Some(LinkEvent::Disconnected(why)) => break why,
            None => break "link closed".to_string(),
        }
    };
    state.send_replace(LinkState::Disconnected(why));
}
