//! A simulated B9X for working on the CLI and GUI without the hardware.
//!
//! It answers the same queries as firmware 0.3.0 and applies writes the way the
//! captures show. It only ever receives frames that passed the whitelist.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use b9x_proto::{self as p, LedSettings, Mode};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::{Link, LinkEvent, LinkRequest};

pub const ADDRESS: &str = "00:00:00:00:B9:0F";
const MAC_REV: [u8; 6] = [0x0f, 0xb9, 0, 0, 0, 0];

struct State {
    product_id: [u8; 7],
    reported_mac: [u8; 6],
    mode: Mode,
    cold: u8,
    fan: u16,
    sub: u8,
    quick: Mode,
    light: bool,
    overclock: bool,
    curve: Vec<u8>,
    effect: HashMap<Mode, u8>,
    leds: HashMap<u8, LedSettings>,
    stored_preset: HashMap<Mode, (u8, u16, u8)>,
    shutdown: (bool, bool, u16),
    shutdown_left: u16,
    temp: f64,
    rpm: f64,
    ticks: u32,
}

const RAINBOW: [p::Rgb; 8] = [
    [0xff, 0, 0],
    [0xff, 0x6a, 0],
    [0xff, 0xff, 0],
    [0, 0xff, 0],
    [0, 0xff, 0xff],
    [0, 0, 0xff],
    [0x9a, 0, 0xff],
    [0xff, 0, 0x80],
];

impl State {
    fn new() -> State {
        let leds = p::EFFECTS
            .iter()
            .map(|e| {
                let colors = match (e.colors, e.fixed_colors) {
                    (Some((1, _)), _) => vec![[0xff, 0xff, 0xff]],
                    (Some(_), _) => RAINBOW.to_vec(),
                    (None, 0) => vec![[0xff, 0, 0]],
                    (None, n) => RAINBOW[..n].to_vec(),
                };
                let s = LedSettings {
                    effect: e.code,
                    speed: e.speeds.map_or(0, |s| s[1]),
                    colors,
                    brightness: 100,
                    color_mode: 1,
                };
                (e.code, s)
            })
            .collect();
        State {
            product_id: b9x_proto::B9X_PRODUCT_ID,
            reported_mac: MAC_REV,
            mode: Mode::Smart,
            cold: 145,
            fan: 4000,
            sub: 0x06,
            quick: Mode::Fixed,
            light: true,
            overclock: false,
            curve: vec![5, 5, 5, 6, 7, 8, 8, 8, 8],
            effect: HashMap::from([(Mode::Smart, 0x0c), (Mode::Fixed, 0x04), (Mode::Custom, 0x03)]),
            leds,
            stored_preset: HashMap::from([(Mode::Smart, (145, 4000, 0x06)), (Mode::Fixed, (145, 4000, 0x00))]),
            shutdown: (false, false, 62),
            shutdown_left: 0,
            temp: 24.0,
            rpm: 0.0,
            ticks: 0,
        }
    }

    fn target(&self) -> (f64, f64) {
        if self.overclock {
            return (-9.0, 5700.0);
        }
        match self.mode {
            Mode::Custom => (8.0 - self.curve.iter().map(|&v| v as f64).sum::<f64>() / 6.0, 4500.0),
            _ => (20.0 - (self.cold as f64 - 120.0) / 4.0, self.fan as f64),
        }
    }

    fn tick(&mut self) -> Vec<u8> {
        let (t, rpm) = self.target();
        self.temp += (t - self.temp) * 0.08;
        self.rpm += (rpm * 0.94 - self.rpm) * 0.3;
        self.ticks += 1;
        if self.shutdown_left > 0 && self.ticks % 60 == 0 {
            self.shutdown_left -= 1;
        }
        let (cold, fan) = if self.mode == Mode::Custom { (160, 4500) } else { (self.cold, self.fan) };
        let tenths = (self.temp.abs() * 10.0).round() as u16;
        let hi = (tenths / 10) as u8;
        let hi = if self.temp < 0.0 { hi.wrapping_neg() } else { hi };
        let rpm = self.rpm.round() as u16;
        let output = if self.overclock { 0xb8 } else { cold.saturating_sub(10) };
        vec![
            0x13,
            0x00,
            self.mode.code() << 4 | 2,
            cold,
            (fan / 100) as u8,
            (fan % 100) as u8,
            if self.mode == Mode::Smart { self.sub } else { 0 },
            hi,
            (tenths % 10) as u8,
            34 + (self.rpm / 1000.0) as u8,
            output,
            (self.rpm / 30.0) as u8,
            (rpm / 100) as u8,
            (rpm % 100) as u8,
            if self.light { 0xff } else { 0 },
            self.effect[&self.mode],
            self.quick.code(),
            0x03 | if self.overclock { 0x80 } else { 0 },
            (self.shutdown_left >> 8) as u8,
            self.shutdown_left as u8,
        ]
    }

    fn led_reply(&self, code: u8) -> Vec<Vec<u8>> {
        let code = if code == 0 { self.effect[&self.mode] } else { code };
        let s = &self.leds[&code];
        p::led_frames(s)
            .into_iter()
            .map(|mut f| {
                f[0] = 0x51;
                f[18] = if s.effect == 0x13 { s.color_mode } else { 1 };
                f[19] = 0;
                f
            })
            .collect()
    }

    /// Apply a write and return the notifications it causes.
    fn write(&mut self, f: &[u8]) -> Vec<Vec<u8>> {
        match f[0] {
            0xa5 => {
                let mut r = vec![0xa5, 0xa0];
                r.extend(self.product_id);
                r.extend(self.reported_mac);
                r.extend([0, 0, 0, 0, 3]);
                return vec![r];
            }
            0x22 => return vec![[&p::QUERY_CURVE[..], &self.curve].concat()],
            0x24 => return vec![[&[0x24][..], &MAC_REV].concat()],
            0x61 => {
                let (r, e, m) = self.shutdown;
                return vec![vec![0x61, r as u8, e as u8, (m >> 8) as u8, m as u8]];
            }
            0x51 => return self.led_reply(f[1]),
            0x10 => self.light = f[1] != 0,
            0x26 => self.overclock = f[1] != 0,
            0x21 => self.quick = Mode::from_code(f[1]).unwrap_or(self.quick),
            0x60 => {
                let m = f[3] as u16 * 256 + f[4] as u16;
                self.shutdown = (f[1] != 0 && f[2] != 0, f[2] != 0, m);
                self.shutdown_left = if f[2] != 0 { m } else { 0 };
            }
            0x20 => {
                let Some(mode) = Mode::from_code(f[1]) else { return vec![] };
                self.mode = mode;
                if mode == Mode::Custom {
                    if f[2] != 0 {
                        self.curve = f[2..11].to_vec();
                    }
                } else {
                    let stored = self.stored_preset[&mode];
                    let (cold, fan, sub) = if f[2] != 0 {
                        (f[2], f[3] as u16 * 100 + f[4] as u16, f[5])
                    } else if mode == Mode::Smart {
                        // A bare Smart tab picks Max or Silent by the sub byte.
                        if f[5] == 0x06 { (145, 4000, 0x06) } else { (175, 5500, 0x00) }
                    } else {
                        stored
                    };
                    self.stored_preset.insert(mode, (cold, fan, sub));
                    (self.cold, self.fan, self.sub) = (cold, fan, sub);
                }
            }
            0x50 => {
                let code = f[1];
                if f[2..19].iter().all(|&b| b == 0) {
                    self.effect.insert(self.mode, code);
                } else {
                    let s = self.leds.get_mut(&code).unwrap();
                    let n = f[3] as usize;
                    let start = (f[4] as usize - 1) * p::COLORS_PER_FRAME;
                    s.colors.resize(n, [0, 0, 0]);
                    for i in 0..p::COLORS_PER_FRAME.min(n - start) {
                        s.colors[start + i] = [f[5 + 3 * i], f[6 + 3 * i], f[7 + 3 * i]];
                    }
                    (s.speed, s.brightness) = (f[2], f[17]);
                    if code == 0x13 {
                        s.color_mode = f[18];
                    }
                }
            }
            _ => {}
        }
        vec![]
    }
}

fn pad(mut f: Vec<u8>) -> Vec<u8> {
    f.resize(f.len().max(20), 0);
    f
}

/// How the simulated device behaves.
#[derive(Debug, Clone)]
pub struct Options {
    /// Time between status pushes.
    pub period: Duration,
    /// Status pushes show the state from this many pushes ago, like the real device
    /// that keeps reporting old values for a moment after a write.
    pub status_lag: usize,
    /// Stored scheduled shutdown (repeat, enable, minutes), e.g. a value over 255
    /// minutes set by the phone app.
    pub shutdown: (bool, bool, u16),
    /// Handshake reply after `a5 a0`, to act like some other device.
    pub product_id: [u8; 7],
    /// MAC the device reports about itself, byte-reversed as sent; normally [`ADDRESS`].
    pub reported_mac: [u8; 6],
}

impl Default for Options {
    fn default() -> Self {
        Options {
            period: Duration::from_secs(1),
            status_lag: 0,
            shutdown: (false, false, 62),
            product_id: b9x_proto::B9X_PRODUCT_ID,
            reported_mac: MAC_REV,
        }
    }
}

/// A link to a new simulated device. Status pushes come every `period`.
pub fn connect(period: Duration) -> Link {
    connect_with(Options { period, ..Options::default() })
}

pub fn connect_with(opts: Options) -> Link {
    let (req_tx, mut req_rx) = mpsc::channel::<LinkRequest>(8);
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut state = State::new();
        state.shutdown = opts.shutdown;
        state.product_id = opts.product_id;
        state.reported_mac = opts.reported_mac;
        state.shutdown_left = if opts.shutdown.1 { opts.shutdown.2 } else { 0 };
        let mut history: VecDeque<Vec<u8>> = VecDeque::new();
        let _ = ev_tx.send(LinkEvent::Notify(pad(b"0.3.0\0".to_vec())));
        let mut ticker = interval(opts.period);
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    history.push_back(state.tick());
                    while history.len() > opts.status_lag + 1 {
                        history.pop_front();
                    }
                    let _ = ev_tx.send(LinkEvent::Notify(history[0].clone()));
                }
                r = req_rx.recv() => match r {
                    Some(LinkRequest::Write(frame, done)) => {
                        let replies = state.write(frame.as_bytes());
                        let _ = done.send(Ok(()));
                        for r in replies {
                            let _ = ev_tx.send(LinkEvent::Notify(pad(r)));
                        }
                    }
                    Some(LinkRequest::Close(done)) => {
                        let _ = ev_tx.send(LinkEvent::Disconnected("closed".into()));
                        let _ = done.send(());
                        return;
                    }
                    None => return,
                },
            }
        }
    });
    Link { address: ADDRESS.into(), requests: req_tx, events: ev_rx }
}
