//! Command line control for the Flydigi B9X phone cooler over BLE.

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::ExitCode;
use std::time::Duration;

use b9x_ble::control::{self, LedChange};
use b9x_ble::{Device, Error, Result, WriteMode, fake};
use b9x_proto::{self as p, LedSettings, Mode, Preset, Rgb, ShutdownInfo, Status};
use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "b9x", version, about = "Command line control for the Flydigi B9X phone cooler over BLE.")]
struct Cli {
    /// Device address. Default: $B9X_ADDRESS or scan
    #[arg(short, long, env = "B9X_ADDRESS", global = true)]
    address: Option<String>,
    #[arg(long, default_value_t = 10.0, global = true)]
    scan_timeout: f64,
    /// Write without response, as the Android app does
    #[arg(long, global = true)]
    write_cmd: bool,
    /// Print frames on stderr
    #[arg(short, long, global = true)]
    debug: bool,
    /// Talk to a simulated device instead (development)
    #[arg(long, global = true)]
    fake: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Smart,
    Fixed,
    Custom,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Mode {
        match m {
            ModeArg::Smart => Mode::Smart,
            ModeArg::Fixed => Mode::Fixed,
            ModeArg::Custom => Mode::Custom,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    Energy,
    Max,
    Silent,
    Strong,
}

impl From<PresetArg> for Preset {
    fn from(p: PresetArg) -> Preset {
        match p {
            PresetArg::Energy => Preset::Energy,
            PresetArg::Max => Preset::Max,
            PresetArg::Silent => Preset::Silent,
            PresetArg::Strong => Preset::Strong,
        }
    }
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum Speed {
    Slow,
    Medium,
    Fast,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Rhythm {
    Single,
    Colorful,
}

#[derive(Subcommand)]
enum Command {
    /// List nearby B9X coolers
    Scan,
    /// Drop a connection left open by a killed b9x (sends nothing to the device)
    Disconnect,
    /// Firmware version and MAC
    Info,
    /// Temperatures, fan, mode
    Status {
        /// Keep printing every second
        #[arg(short, long)]
        watch: bool,
        #[arg(long)]
        json: bool,
    },
    /// Set cooling mode
    Mode {
        mode: ModeArg,
        /// smart: max, silent. fixed: energy, strong, max
        #[arg(long)]
        preset: Option<PresetArg>,
        /// custom: 9 levels 1-8 for -12,-9,-6,-3,0,3,6,9,12 °C, comma separated
        #[arg(long, value_parser = curve_arg)]
        curve: Option<Curve>,
    },
    /// Mode the power button toggles to on long press
    QuickSwitch { mode: Option<ModeArg> },
    /// Show the custom mode curve
    Curve,
    /// Lighting on/off
    Light { state: Option<OnOff> },
    /// OC Experience (full power, frost protection off). The cooler ends OC when the
    /// connection drops, so `oc on` stays connected and shows status until Ctrl-C
    Oc {
        state: Option<OnOff>,
        #[arg(long)]
        yes: bool,
    },
    /// Scheduled shutdown
    Shutdown {
        /// e.g. 90 or 1h30m (max 4h15m)
        #[arg(value_parser = minutes_arg, conflicts_with = "off")]
        minutes: Option<u16>,
        #[arg(long)]
        off: bool,
        /// Repeat every time instead of once
        #[arg(long)]
        repeat: bool,
    },
    /// Show or set the LED effect of the current mode
    Led {
        #[arg(value_parser = PossibleValuesParser::new(p::EFFECTS.map(|e| e.name)))]
        effect: Option<String>,
        #[arg(long)]
        speed: Option<Speed>,
        #[arg(long, num_args = 1.., value_name = "RRGGBB", value_parser = color_arg)]
        colors: Option<Vec<Rgb>>,
        /// 20-100
        #[arg(long, value_parser = brightness_arg)]
        brightness: Option<u8>,
        #[arg(long)]
        rhythm: Option<Rhythm>,
    },
}

// --- argument types ----------------------------------------------------------

#[derive(Clone)]
struct Curve(Vec<u8>);

fn curve_arg(text: &str) -> Result<Curve, String> {
    let levels =
        text.split(',').map(|v| v.trim().parse::<u8>()).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    p::custom_curve(&levels).map_err(|e| e.0)?;
    Ok(Curve(levels))
}

fn minutes_arg(text: &str) -> Result<u16, String> {
    let bad = || "use minutes or e.g. 1h30m".to_string();
    let num = |s: &str| s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse::<u32>().ok()).flatten();
    let (hours, rest) = match text.split_once('h') {
        Some((h, rest)) => (num(h).ok_or_else(bad)?, rest),
        None if text.is_empty() => return Err(bad()),
        None => (0, text),
    };
    let mins = if rest.is_empty() { 0 } else { num(rest.strip_suffix('m').unwrap_or(rest)).ok_or_else(bad)? };
    let range = || format!("1-{} minutes", p::SHUTDOWN_MAX_MIN);
    let total = hours.checked_mul(60).and_then(|h| h.checked_add(mins)).ok_or_else(range)?;
    if !(1..=p::SHUTDOWN_MAX_MIN as u32).contains(&total) {
        return Err(range());
    }
    Ok(total as u16)
}

fn color_arg(text: &str) -> Result<Rgb, String> {
    let t = text.trim_start_matches('#');
    if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("colour must be RRGGBB".into());
    }
    let b = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).unwrap();
    Ok([b(0), b(2), b(4)])
}

fn brightness_arg(text: &str) -> Result<u8, String> {
    let range = format!("{}-{}", p::BRIGHTNESS_MIN, p::BRIGHTNESS_MAX);
    let v: u8 = text.parse().map_err(|_| range.clone())?;
    if !(p::BRIGHTNESS_MIN..=p::BRIGHTNESS_MAX).contains(&v) {
        return Err(range);
    }
    Ok(v)
}

// --- output ------------------------------------------------------------------

fn hexcolor(c: &Rgb) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn effect_label(code: u8) -> String {
    match p::effect_by_code(code) {
        Some(e) => format!("{} ({})", e.name, e.label),
        None => format!("unknown {code:02x}"),
    }
}

fn status_json(s: &Status) -> serde_json::Value {
    let mode = |m: u8| Mode::from_code(m).map_or(serde_json::json!(m), |m| serde_json::json!(m.name()));
    serde_json::json!({
        "mode": mode(s.mode),
        "preset": s.preset().map(Preset::name),
        "cold_setpoint": s.cold_setpoint,
        "fan_setpoint": s.fan_setpoint,
        "cold_temp": s.cold_temp,
        "hot_temp": s.hot_temp,
        "output": s.output,
        "fan_rpm": s.fan_rpm,
        "light": s.light,
        "effect": p::effect_by_code(s.effect).map_or(serde_json::json!(s.effect), |e| serde_json::json!(e.name)),
        "quick_switch": mode(s.quick_switch),
        "overclock": s.overclock,
        "shutdown_left": s.shutdown_left,
    })
}

fn status_text(s: &Status) -> String {
    let preset = s.preset().map_or(String::new(), |pr| format!(" / {}", pr.name()));
    let mut t = String::new();
    let _ = writeln!(t, "mode          {}{}", p::mode_name(s.mode), preset);
    let _ = writeln!(t, "setpoint      cold {}, fan {} rpm", s.cold_setpoint, s.fan_setpoint);
    let _ = writeln!(t, "cold plate    {:.1} °C", s.cold_temp);
    let _ = writeln!(t, "hot side      {} °C (estimated)", s.hot_temp);
    let _ = writeln!(t, "output        {}", s.output);
    let _ = writeln!(t, "fan           {} rpm", s.fan_rpm);
    let _ = writeln!(t, "light         {}, effect {}", on_off(s.light), effect_label(s.effect));
    let _ = writeln!(t, "quick switch  {}", p::mode_name(s.quick_switch));
    let _ = writeln!(t, "OC            {}", on_off(s.overclock));
    if s.shutdown_left > 0 {
        let _ = writeln!(t, "shutdown in   {}h{:02}m", s.shutdown_left / 60, s.shutdown_left % 60);
    }
    t
}

fn led_text(s: &LedSettings) -> String {
    let mut t = String::new();
    let _ = writeln!(t, "effect      {}", effect_label(s.effect));
    let e = p::effect_by_code(s.effect);
    if e.is_some_and(|e| e.speeds.is_some()) {
        let _ = writeln!(t, "speed       {} ({})", s.speed_name().unwrap_or("?"), s.speed);
    }
    if e.is_some_and(|e| e.colors.is_some() || e.fixed_colors > 0) {
        let _ = writeln!(t, "colors      {}", s.colors.iter().map(hexcolor).collect::<Vec<_>>().join(" "));
    }
    let _ = writeln!(t, "brightness  {}", s.brightness);
    if e.is_some_and(|e| e.name == "rhythm") {
        let mode = if s.color_mode == p::RHYTHM_COLORFUL { "colorful" } else { "single" };
        let _ = writeln!(t, "rhythm      {mode}");
    }
    t
}

fn shutdown_text(sd: &ShutdownInfo) -> String {
    if !sd.enable {
        return "scheduled shutdown off\n".into();
    }
    let kind = if sd.repeat { "repeat" } else { "once" };
    format!("scheduled shutdown in {}h{:02}m ({kind})\n", sd.minutes / 60, sd.minutes % 60)
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

// --- commands ----------------------------------------------------------------

enum CliError {
    Device(Error),
    /// Stdout was closed, e.g. by `| head`.
    Closed,
}

impl From<Error> for CliError {
    fn from(e: Error) -> Self {
        CliError::Device(e)
    }
}

fn out(text: impl AsRef<str>) -> Result<(), CliError> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(text.as_ref().as_bytes()).and_then(|()| stdout.flush()).map_err(|_| CliError::Closed)
}

fn line(text: impl std::fmt::Display) -> String {
    format!("{text}\n")
}

async fn run(cli: &Cli, dev: &Device) -> Result<(), CliError> {
    match &cli.command {
        Command::Scan | Command::Disconnect => unreachable!(),
        Command::Info => {
            // BlueZ drops the version push: it arrives before the CCCD write is acknowledged.
            out(line(format_args!("firmware  {}", dev.version().as_deref().unwrap_or("?"))))?;
            out(line(format_args!("mac       {}", dev.mac().await?)))?;
        }
        Command::Status { watch, json } => loop {
            let s = dev.status().await?;
            if *json {
                out(line(status_json(&s)))?;
            } else {
                out(status_text(&s))?;
            }
            if !watch {
                break;
            }
            if !json {
                out("\n")?;
            }
        },
        Command::Mode { mode, preset, curve } => {
            let curve = curve.as_ref().map(|c| c.0.as_slice());
            control::set_mode(dev, (*mode).into(), preset.map(Into::into), curve).await?;
            tokio::time::sleep(Duration::from_millis(1500)).await;
            out(status_text(&dev.status().await?))?;
        }
        Command::QuickSwitch { mode: None } => out(line(p::mode_name(dev.status().await?.quick_switch)))?,
        Command::QuickSwitch { mode: Some(m) } => control::set_quick_switch(dev, (*m).into()).await?,
        Command::Curve => {
            let mut t = String::new();
            for (temp, v) in p::CURVE_TEMPS.iter().zip(dev.curve().await?) {
                let _ = writeln!(t, "{temp:4} °C  {v} {}", "#".repeat(v as usize));
            }
            out(t)?;
        }
        Command::Light { state: None } => out(line(on_off(dev.status().await?.light)))?,
        Command::Light { state: Some(s) } => control::set_light(dev, *s == OnOff::On).await?,
        Command::Oc { state: None, .. } => out(line(on_off(dev.status().await?.overclock)))?,
        Command::Oc { state: Some(OnOff::On), yes } => {
            control::set_overclock(dev, true, *yes).await?;
            eprintln!("OC on until Ctrl-C (the cooler ends it when the connection drops)");
            loop {
                let s = dev.status().await?;
                let oc = if s.overclock { "" } else { "  (OC off)" };
                out(line(format_args!(
                    "cold plate {:5.1} °C  output {:3}  fan {} rpm{oc}",
                    s.cold_temp, s.output, s.fan_rpm
                )))?;
            }
        }
        Command::Oc { state: Some(OnOff::Off), .. } => control::set_overclock(dev, false, true).await?,
        Command::Shutdown { minutes: None, off: false, .. } => out(shutdown_text(&dev.shutdown().await?))?,
        Command::Shutdown { minutes, off, repeat } => {
            let schedule = if *off { None } else { minutes.map(|m| (m, *repeat)) };
            out(shutdown_text(&control::set_shutdown(dev, schedule).await?))?;
        }
        Command::Led { effect: None, .. } => out(led_text(&dev.led(0).await?))?,
        Command::Led { effect: Some(name), speed, colors, brightness, rhythm } => {
            let e = p::effect_by_name(name).expect("clap checked the name");
            let change = LedChange {
                speed: speed.map(|s| s as usize),
                colors: colors.clone(),
                brightness: *brightness,
                rhythm_colorful: rhythm.map(|r| r == Rhythm::Colorful),
            };
            out(led_text(&control::set_led(dev, e, &change).await?))?;
        }
    }
    Ok(())
}

async fn find_address(cli: &Cli) -> Result<Option<String>> {
    if cli.fake {
        return Ok(None);
    }
    match &cli.address {
        Some(a) => Ok(Some(a.clone())),
        None => Ok(Some(b9x_ble::find(Duration::from_secs_f64(cli.scan_timeout)).await?)),
    }
}

async fn open(cli: &Cli, address: Option<&str>) -> Result<Device> {
    let Some(address) = address else {
        return Device::open(fake::connect(Duration::from_secs(1)), cli.debug).await;
    };
    let mode = if cli.write_cmd { WriteMode::Command } else { WriteMode::Request };
    let timeout = Duration::from_secs_f64(cli.scan_timeout);
    Device::open(b9x_ble::connect(address, mode, timeout).await?, cli.debug).await
}

/// Resolves on Ctrl-C, SIGTERM or SIGHUP so the connection is closed on the way out.
/// BlueZ keeps the link up after the process dies, which locks out the phone app.
async fn interrupted() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::hangup())) else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
            _ = hup.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

async fn main_async(cli: Cli) -> Result<(), CliError> {
    let timeout = Duration::from_secs_f64(cli.scan_timeout);
    match (&cli.command, cli.fake) {
        (Command::Scan, true) => return out(line(format_args!("{}  {:<16} rssi 0", fake::ADDRESS, "fake B9X"))),
        (Command::Scan, false) => {
            for d in b9x_ble::scan(timeout).await? {
                out(line(format_args!("{}  {:<16} rssi {}", d.address, d.name, d.rssi.unwrap_or(0))))?;
            }
            return Ok(());
        }
        // The simulated device has no BlueZ link to drop.
        (Command::Disconnect, true) => return Ok(()),
        (Command::Disconnect, false) => {
            for addr in b9x_ble::disconnect(cli.address.as_deref()).await? {
                out(line(format_args!("disconnected {addr}")))?;
            }
            return Ok(());
        }
        _ => {}
    }
    // Scanning holds no link, so an interrupt there needs no cleanup.
    let address = tokio::select! {
        a = find_address(&cli) => a?,
        _ = interrupted() => return Ok(()),
    };
    let dev = tokio::select! {
        d = open(&cli, address.as_deref()) => d?,
        _ = interrupted() => {
            // The connect may have got through in BlueZ before it was dropped here.
            if let Some(a) = &address {
                let _ = b9x_ble::disconnect(Some(a)).await;
            }
            return Ok(());
        }
    };
    if let Some(w) = dev.identity_warning() {
        eprintln!("b9x: warning: {w}");
    }
    let mut work = std::pin::pin!(run(&cli, &dev));
    let result = tokio::select! {
        r = &mut work => r,
        _ = interrupted() => match cli.command {
            Command::Status { .. } | Command::Oc { state: Some(OnOff::On), .. } => Ok(()),
            // Let a change of several frames finish so the device is not left half
            // set. Its replies time out within seconds; a second signal stops at once.
            _ => tokio::select! {
                r = &mut work => r,
                _ = interrupted() => Ok(()),
            },
        },
    };
    if let Command::Oc { state: Some(OnOff::On), yes: true } = cli.command {
        // Disconnecting ends OC too, but say so explicitly while we still can.
        let _ = control::set_overclock(&dev, false, true).await;
    }
    dev.disconnect().await;
    result
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut cli = Cli::parse();
    // An empty $B9X_ADDRESS means scan, as before.
    cli.address = cli.address.filter(|a| !a.is_empty());
    match main_async(cli).await {
        Ok(()) | Err(CliError::Closed) => ExitCode::SUCCESS,
        Err(CliError::Device(e)) => {
            eprintln!("b9x: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn minutes() {
        for (text, want) in [("90", 90), ("90m", 90), ("1h", 60), ("1h30", 90), ("1h30m", 90), ("4h15m", 255)] {
            assert_eq!(minutes_arg(text), Ok(want), "{text}");
        }
        for text in ["", "m", "h", "1hm", "0", "4h16m", "1h-3", "x", "71582789h", "4294967295m", "99999999999h"] {
            assert!(minutes_arg(text).is_err(), "{text}");
        }
    }

    #[test]
    fn colours_and_curve() {
        assert_eq!(color_arg("#ff0080"), Ok([0xff, 0, 0x80]));
        assert!(color_arg("ff008").is_err());
        assert!(curve_arg("5,5,5,6,7,8,8,8,8").is_ok());
        assert!(curve_arg("5,5,5,6,7,8,8,8,9").is_err());
    }

    #[test]
    fn parser() {
        Cli::command().debug_assert();
    }
}
