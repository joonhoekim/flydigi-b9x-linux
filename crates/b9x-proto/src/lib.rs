//! B9X frame codec and the whitelist of frames the official app was seen sending.
//!
//! Every frame that goes to the device is a [`Frame`], and the only way to get one is
//! [`Frame::check`]. It accepts the opcodes and value ranges recorded in
//! docs/protocol.md; anything else is an [`UnsafeFrame`]. Some Flydigi firmware erases
//! itself on unknown opcodes, so there is deliberately no way to send an arbitrary frame.

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

pub const SERVICE_UUID: u128 = 0x0000ffe0_0000_1000_8000_00805f9b34fb;
pub const WRITE_UUID: u128 = 0x0000ffe1_0000_1000_8000_00805f9b34fb;
pub const NOTIFY_UUID: u128 = 0x0000ffe4_0000_1000_8000_00805f9b34fb;

/// The frame is not one the official app sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeFrame(pub String);

impl fmt::Display for UnsafeFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnsafeFrame {}

/// A builder was given a value outside the range the app uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueError(pub String);

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ValueError {}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

// --- modes -------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Smart = 0x00,
    Fixed = 0x01,
    Custom = 0x04,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Smart, Mode::Fixed, Mode::Custom];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.code() == code)
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Smart => "smart",
            Mode::Fixed => "fixed",
            Mode::Custom => "custom",
        }
    }

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// Name of a raw mode byte, or its number when unknown.
pub fn mode_name(code: u8) -> String {
    Mode::from_code(code).map_or_else(|| code.to_string(), |m| m.name().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    Max,
    Silent,
    Energy,
    Strong,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Energy, Preset::Max, Preset::Silent, Preset::Strong];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Max => "max",
            Preset::Silent => "silent",
            Preset::Energy => "energy",
            Preset::Strong => "strong",
        }
    }

    pub fn from_name(name: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetValues {
    pub mode: Mode,
    pub preset: Preset,
    pub cold: u8,
    pub fan: u16,
    /// Smart sub-preset byte.
    pub sub: u8,
}

pub const PRESETS: [PresetValues; 5] = [
    PresetValues { mode: Mode::Smart, preset: Preset::Max, cold: 175, fan: 5500, sub: 0x00 },
    PresetValues { mode: Mode::Smart, preset: Preset::Silent, cold: 145, fan: 4000, sub: 0x06 },
    PresetValues { mode: Mode::Fixed, preset: Preset::Energy, cold: 145, fan: 4000, sub: 0x00 },
    PresetValues { mode: Mode::Fixed, preset: Preset::Strong, cold: 155, fan: 4500, sub: 0x00 },
    PresetValues { mode: Mode::Fixed, preset: Preset::Max, cold: 175, fan: 5700, sub: 0x00 },
];

pub fn preset_values(mode: Mode, preset: Preset) -> Option<&'static PresetValues> {
    PRESETS.iter().find(|p| p.mode == mode && p.preset == preset)
}

pub fn presets_for(mode: Mode) -> impl Iterator<Item = Preset> {
    PRESETS.iter().filter(move |p| p.mode == mode).map(|p| p.preset)
}

pub const CURVE_TEMPS: [i8; 9] = [-12, -9, -6, -3, 0, 3, 6, 9, 12];
pub const CURVE_MIN: u8 = 1;
pub const CURVE_MAX: u8 = 8;

fn setpoint(cold: u8, fan: u16) -> [u8; 3] {
    [cold, (fan / 100) as u8, (fan % 100) as u8]
}

pub fn mode_preset(mode: Mode, preset: Preset) -> Result<Vec<u8>, ValueError> {
    let p = preset_values(mode, preset)
        .ok_or_else(|| ValueError(format!("{} has no preset {}", mode.name(), preset.name())))?;
    let sp = setpoint(p.cold, p.fan);
    let mut f = vec![0x20, mode.code()];
    f.extend(sp);
    f.push(p.sub);
    f.extend(sp);
    Ok(f)
}

/// Switch mode and keep the preset or curve stored for it.
pub fn mode_tab(mode: Mode) -> Result<Vec<u8>, ValueError> {
    let zeros = match mode {
        Mode::Fixed => 7,
        Mode::Custom => 9,
        Mode::Smart => return Err(ValueError("smart needs a preset".into())),
    };
    let mut f = vec![0x20, mode.code()];
    f.resize(2 + zeros, 0);
    Ok(f)
}

pub fn custom_curve(levels: &[u8]) -> Result<Vec<u8>, ValueError> {
    if levels.len() != CURVE_TEMPS.len() || !levels.iter().all(|v| (CURVE_MIN..=CURVE_MAX).contains(v)) {
        return Err(ValueError(format!("curve needs {} levels, each {}-{}", CURVE_TEMPS.len(), CURVE_MIN, CURVE_MAX)));
    }
    let mut f = vec![0x20, Mode::Custom.code()];
    f.extend_from_slice(levels);
    Ok(f)
}

pub fn quick_switch(mode: Mode) -> Vec<u8> {
    vec![0x21, mode.code()]
}

pub const HANDSHAKE: [u8; 4] = [0xa5, 0xa0, 0x00, 0x00];
pub const QUERY_CURVE: [u8; 2] = [0x22, 0x04];
pub const QUERY_MAC: [u8; 2] = [0x24, 0x00];
pub const QUERY_SHUTDOWN: [u8; 1] = [0x61];

pub fn light(on: bool) -> Vec<u8> {
    vec![0x10, if on { 0xff } else { 0x00 }]
}

pub fn overclock(on: bool) -> Vec<u8> {
    vec![0x26, on as u8, 0x00, 0x00]
}

/// Minutes go out as two bytes. Only values below 256 were captured, so the
/// high byte stays 0 until its byte order is confirmed.
pub const SHUTDOWN_MAX_MIN: u16 = 255;

pub fn shutdown(repeat: bool, enable: bool, minutes: u16) -> Result<Vec<u8>, ValueError> {
    if minutes > SHUTDOWN_MAX_MIN || (enable && minutes == 0) {
        return Err(ValueError(format!("minutes must be 1-{SHUTDOWN_MAX_MIN}")));
    }
    Ok(vec![0x60, repeat as u8, enable as u8, 0x00, minutes as u8])
}

// --- LED ---------------------------------------------------------------------

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub code: u8,
    pub name: &'static str,
    pub label: &'static str,
    /// (min, max) colours the user may set; None = not settable.
    pub colors: Option<(usize, usize)>,
    /// Speed byte for slow, medium, fast; None = no speed.
    pub speeds: Option<[u8; 3]>,
    /// False: the app only ever selected it.
    pub writable: bool,
    /// Colours the app passes through unchanged.
    pub fixed_colors: usize,
}

const fn effect(code: u8, name: &'static str, label: &'static str) -> Effect {
    Effect { code, name, label, colors: None, speeds: None, writable: true, fixed_colors: 0 }
}

pub const EFFECTS: [Effect; 11] = [
    Effect { writable: false, ..effect(0x0c, "smart", "Smart Control") },
    Effect { speeds: Some([6, 4, 2]), fixed_colors: 1, ..effect(0x03, "flow", "Flow") },
    Effect { colors: Some((2, 8)), speeds: Some([10, 8, 6]), ..effect(0x06, "gradient", "Gradient") },
    Effect { colors: Some((2, 8)), speeds: Some([8, 5, 3]), ..effect(0x05, "breathing", "Breathing") },
    Effect { colors: Some((2, 8)), speeds: Some([8, 5, 3]), ..effect(0x04, "cutover", "Cutover") },
    Effect { colors: Some((1, 1)), ..effect(0x0b, "always-on", "Always on") },
    Effect { colors: Some((2, 8)), speeds: Some([8, 5, 3]), ..effect(0x10, "rotation", "Multi-color Rotation") },
    Effect { writable: false, ..effect(0x11, "pulsating", "Pulsating") },
    Effect { colors: Some((1, 1)), ..effect(0x13, "rhythm", "Rhythm") },
    Effect { writable: false, ..effect(0x14, "cyberpunk", "Cyberpunk") },
    Effect { colors: Some((2, 8)), speeds: Some([3, 5, 8]), ..effect(0x15, "collision", "Collision") },
];

pub fn effect_by_code(code: u8) -> Option<&'static Effect> {
    EFFECTS.iter().find(|e| e.code == code)
}

pub fn effect_by_name(name: &str) -> Option<&'static Effect> {
    EFFECTS.iter().find(|e| e.name == name)
}

pub const SPEED_NAMES: [&str; 3] = ["slow", "medium", "fast"];
pub const BRIGHTNESS_MIN: u8 = 20;
pub const BRIGHTNESS_MAX: u8 = 100;
pub const RHYTHM_SINGLE: u8 = 0x01;
pub const RHYTHM_COLORFUL: u8 = 0x02;
pub const COLORS_PER_FRAME: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LedSettings {
    pub effect: u8,
    pub speed: u8,
    pub colors: Vec<Rgb>,
    pub brightness: u8,
    /// Rhythm only.
    pub color_mode: u8,
}

impl LedSettings {
    pub fn speed_name(&self) -> Option<&'static str> {
        let speeds = effect_by_code(self.effect)?.speeds?;
        speeds.iter().position(|&s| s == self.speed).map(|i| SPEED_NAMES[i])
    }
}

pub fn checksum(frame: &[u8]) -> u8 {
    frame.iter().fold(0u8, |a, &b| a.wrapping_add(b))
}

fn with_checksum(mut frame: Vec<u8>) -> Vec<u8> {
    frame.push(checksum(&frame));
    frame
}

/// Switch effect; the device applies the settings stored for it.
pub fn led_select(code: u8) -> Vec<u8> {
    let mut f = vec![0x50, code];
    f.resize(19, 0);
    with_checksum(f)
}

pub fn led_query(code: u8) -> Vec<u8> {
    vec![0x51, code]
}

/// Full settings write, split into 4-colour frames like the app does.
pub fn led_frames(s: &LedSettings) -> Vec<Vec<u8>> {
    s.colors
        .chunks(COLORS_PER_FRAME)
        .enumerate()
        .map(|(i, chunk)| {
            let mut f = vec![0x50, s.effect, s.speed, s.colors.len() as u8, i as u8 + 1];
            let mut rgb: Vec<u8> = chunk.iter().flatten().copied().collect();
            rgb.resize(12, 0);
            f.extend(rgb);
            f.extend([s.brightness, s.color_mode]);
            with_checksum(f)
        })
        .collect()
}

/// Join one or two `51` replies into LedSettings.
pub fn parse_led_reply(frames: &[Vec<u8>]) -> LedSettings {
    let first = &frames[0];
    let n = first[3] as usize;
    let mut sorted: Vec<&Vec<u8>> = frames.iter().collect();
    sorted.sort_by_key(|f| f[4]);
    let mut colors = Vec::new();
    for f in sorted {
        for i in 0..COLORS_PER_FRAME {
            if colors.len() < n {
                colors.push([f[5 + 3 * i], f[6 + 3 * i], f[7 + 3 * i]]);
            }
        }
    }
    LedSettings { effect: first[1], speed: first[2], colors, brightness: first[17], color_mode: first[18] }
}

pub fn led_frames_needed(reply: &[u8]) -> usize {
    if reply[3] as usize > COLORS_PER_FRAME { 2 } else { 1 }
}

// --- status ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub mode: u8,
    pub cold_setpoint: u8,
    pub fan_setpoint: u16,
    pub smart_preset: u8,
    pub cold_temp: f64,
    pub hot_temp: u8,
    pub output: u8,
    pub fan_rpm: u16,
    pub light: bool,
    pub effect: u8,
    pub quick_switch: u8,
    pub overclock: bool,
    /// Minutes until scheduled shutdown, 0 = off.
    pub shutdown_left: u16,
}

impl Status {
    pub fn preset(&self) -> Option<Preset> {
        let mode = Mode::from_code(self.mode)?;
        let exact = PRESETS.iter().find(|p| {
            p.mode == mode
                && (p.cold, p.fan) == (self.cold_setpoint, self.fan_setpoint)
                && (mode != Mode::Smart || p.sub == self.smart_preset)
        });
        if let Some(p) = exact {
            return Some(p.preset);
        }
        match (mode, self.smart_preset) {
            (Mode::Smart, 0x00) => Some(Preset::Max),
            (Mode::Smart, 0x06) => Some(Preset::Silent),
            _ => None,
        }
    }
}

pub fn cold_temp(hi: u8, lo: u8) -> f64 {
    let t = (hi as i8 as f64).abs() + lo as f64 / 10.0;
    if hi >= 0x80 { -t } else { t }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn parse_status(f: &[u8]) -> Result<Status, ValueError> {
    if f.len() < 18 || f[0] != 0x13 {
        return Err(ValueError("not a status frame".into()));
    }
    Ok(Status {
        mode: f[2] >> 4,
        cold_setpoint: f[3],
        fan_setpoint: f[4] as u16 * 100 + f[5] as u16,
        smart_preset: f[6],
        cold_temp: round1(cold_temp(f[7], f[8])),
        hot_temp: f[9],
        output: f[10],
        fan_rpm: f[12] as u16 * 100 + f[13] as u16,
        light: f[14] != 0,
        effect: f[15],
        quick_switch: f[16],
        overclock: f[17] & 0x80 != 0,
        shutdown_left: if f.len() >= 20 { f[18] as u16 * 256 + f[19] as u16 } else { 0 },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownInfo {
    pub repeat: bool,
    pub enable: bool,
    pub minutes: u16,
}

/// `61` reply.
pub fn parse_shutdown(f: &[u8]) -> Result<ShutdownInfo, ValueError> {
    if f.len() < 5 || f[0] != 0x61 {
        return Err(ValueError("not a shutdown reply".into()));
    }
    Ok(ShutdownInfo { repeat: f[1] != 0, enable: f[2] != 0, minutes: f[3] as u16 * 256 + f[4] as u16 })
}

pub fn parse_curve(f: &[u8]) -> Result<Vec<u8>, ValueError> {
    f.get(2..2 + CURVE_TEMPS.len()).map(<[u8]>::to_vec).ok_or_else(|| ValueError("short curve reply".into()))
}

fn mac_text(reversed: &[u8]) -> String {
    reversed.iter().rev().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

pub fn parse_mac(f: &[u8]) -> Result<String, ValueError> {
    Ok(mac_text(f.get(1..7).ok_or_else(|| ValueError("short MAC reply".into()))?))
}

/// Bytes after `a5 a0` in the handshake reply of the one B9X seen so far. BS3 Pro
/// coolers send `19 31 …`, so this looks like a product id; other B9X units may differ.
pub const B9X_PRODUCT_ID: [u8; 7] = [0x19, 0x30, 0xf0, 0xf1, 0xf2, 0xf3, 0xf4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    pub product_id: [u8; 7],
    /// The device's own address, sent byte-reversed.
    pub mac: String,
}

/// `a5 a0 <product id × 7> <MAC reversed × 6> …` reply.
pub fn parse_handshake(f: &[u8]) -> Result<Handshake, ValueError> {
    if f.len() < 15 || f[..2] != HANDSHAKE[..2] {
        return Err(ValueError("not a handshake reply".into()));
    }
    let mut product_id = [0; 7];
    product_id.copy_from_slice(&f[2..9]);
    Ok(Handshake { product_id, mac: mac_text(&f[9..15]) })
}

// --- whitelist ---------------------------------------------------------------

/// A frame that passed the whitelist. Transports only accept this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame(Vec<u8>);

impl Frame {
    pub fn check(bytes: impl Into<Vec<u8>>) -> Result<Frame, UnsafeFrame> {
        let bytes = bytes.into();
        check_frame(&bytes)?;
        Ok(Frame(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

static EXACT: LazyLock<HashSet<Vec<u8>>> = LazyLock::new(|| {
    let mut s: HashSet<Vec<u8>> = HashSet::new();
    s.extend([HANDSHAKE.to_vec(), QUERY_CURVE.to_vec(), QUERY_MAC.to_vec(), QUERY_SHUTDOWN.to_vec()]);
    s.extend([light(true), light(false), overclock(true), overclock(false)]);
    s.extend([Mode::Fixed, Mode::Custom].map(|m| mode_tab(m).unwrap()));
    s.extend(PRESETS.iter().map(|p| mode_preset(p.mode, p.preset).unwrap()));
    // Tab switches the app sent with the Smart sub-preset byte left over.
    for m in [Mode::Smart, Mode::Fixed] {
        for x in [0x00, 0x06] {
            s.insert(vec![0x20, m.code(), 0, 0, 0, x, 0, 0, 0]);
        }
    }
    s.extend(Mode::ALL.map(quick_switch));
    s.insert(led_query(0));
    s.extend(EFFECTS.iter().map(|e| led_query(e.code)));
    s.extend(EFFECTS.iter().map(|e| led_select(e.code)));
    s
});

fn unsafe_frame<T>(msg: String) -> Result<T, UnsafeFrame> {
    Err(UnsafeFrame(msg))
}

fn check_led(f: &[u8]) -> Result<(), UnsafeFrame> {
    if f.len() != 20 || checksum(&f[..19]) != f[19] {
        return unsafe_frame("bad LED frame length or checksum".into());
    }
    let e = match effect_by_code(f[1]) {
        Some(e) if e.writable => e,
        _ => return unsafe_frame(format!("effect {:02x} only takes a select frame", f[1])),
    };
    let (speed, n, pkt, bright, cmode) = (f[2], f[3] as usize, f[4] as usize, f[17], f[18]);
    let speeds: &[u8] = match &e.speeds {
        Some(s) => s,
        None => &[0],
    };
    if !speeds.contains(&speed) {
        return unsafe_frame(format!("speed {speed} not used by the app for {}", e.name));
    }
    let (lo, hi) = e.colors.unwrap_or((e.fixed_colors, e.fixed_colors));
    if !(lo..=hi).contains(&n) {
        return unsafe_frame(format!("{n} colours not allowed for {}", e.name));
    }
    let frames = n.div_ceil(COLORS_PER_FRAME);
    if !(1..=frames).contains(&pkt) {
        return unsafe_frame(format!("packet number {pkt} out of range"));
    }
    let used = COLORS_PER_FRAME.min(n - (pkt - 1) * COLORS_PER_FRAME);
    if f[5 + 3 * used..17].iter().any(|&b| b != 0) {
        return unsafe_frame("unused colour slots must be zero".into());
    }
    if !(BRIGHTNESS_MIN..=BRIGHTNESS_MAX).contains(&bright) {
        return unsafe_frame(format!("brightness {bright} out of range"));
    }
    let allowed: &[u8] = if e.name == "rhythm" { &[RHYTHM_SINGLE, RHYTHM_COLORFUL] } else { &[0] };
    if !allowed.contains(&cmode) {
        return unsafe_frame(format!("colour mode {cmode} not used by the app for {}", e.name));
    }
    Ok(())
}

pub fn check_frame(f: &[u8]) -> Result<(), UnsafeFrame> {
    if EXACT.contains(f) {
        return Ok(());
    }
    if f.len() == 11 && f[..2] == [0x20, Mode::Custom.code()] {
        if f[2..].iter().all(|v| (CURVE_MIN..=CURVE_MAX).contains(v)) {
            return Ok(());
        }
        return unsafe_frame(format!("curve levels out of range: {}", hex(f)));
    }
    if f.len() == 5 && f[0] == 0x60 {
        if f[1] <= 1 && f[2] <= 1 && f[3] == 0 && (f[4] != 0 || f[2] == 0) {
            return Ok(());
        }
        return unsafe_frame(format!("shutdown frame out of range: {}", hex(f)));
    }
    if f.first() == Some(&0x50) {
        return check_led(f);
    }
    unsafe_frame(format!("frame not on the whitelist: {}", hex(f)))
}
