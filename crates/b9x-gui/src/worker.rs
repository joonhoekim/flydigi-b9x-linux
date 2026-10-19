//! The tokio side: owns the connection and runs one command at a time.
//!
//! The GTK main loop sends [`Cmd`]s and receives [`Event`]s over async channels, so
//! no BLE work ever blocks the UI.

use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use b9x_ble::control::{self, LedChange};
use b9x_ble::{Device, LinkState, WriteMode, fake};
use b9x_proto::{LedSettings, Mode, Preset, ShutdownInfo, Status};
use tokio::sync::oneshot;

pub const SCAN_TIMEOUT: Duration = Duration::from_secs(10);

pub enum Cmd {
    Connect {
        address: Option<String>,
        fake: bool,
    },
    Disconnect,
    Mode(Mode, Option<Preset>),
    Curve(Vec<u8>),
    QuickSwitch(Mode),
    Light(bool),
    /// Only sent after the user confirmed the warning.
    Overclock(bool),
    Shutdown(Option<(u16, bool)>),
    Led(u8, LedChange),
    RefreshLed,
    Quit(oneshot::Sender<()>),
}

pub enum Event {
    Connecting(String),
    Connected {
        address: String,
        version: Option<String>,
        mac: String,
    },
    /// None when the user asked for it.
    Disconnected(Option<String>),
    Status(Status),
    Led(LedSettings),
    Curve(Vec<u8>),
    Shutdown(ShutdownInfo),
    Error(String),
    /// SIGINT, SIGTERM or SIGHUP arrived.
    Quit,
}

pub async fn run(cmds: Receiver<Cmd>, events: Sender<Event>) {
    let mut dev: Option<Arc<Device>> = None;
    while let Ok(cmd) = cmds.recv().await {
        if let Cmd::Quit(done) = cmd {
            if let Some(d) = dev.take() {
                d.disconnect().await;
            }
            let _ = done.send(());
            return;
        }
        if let Err(e) = handle(&mut dev, cmd, &events).await {
            let _ = events.send(Event::Error(e.to_string())).await;
        }
    }
}

fn connected(dev: &Option<Arc<Device>>) -> b9x_ble::Result<&Device> {
    dev.as_deref().ok_or_else(|| b9x_ble::Error::Disconnected("not connected".into()))
}

async fn handle(dev: &mut Option<Arc<Device>>, cmd: Cmd, events: &Sender<Event>) -> b9x_ble::Result<()> {
    let send = |e: Event| async move {
        let _ = events.send(e).await;
    };
    match cmd {
        Cmd::Connect { address, fake } => {
            if let Some(d) = dev.take() {
                d.disconnect().await;
            }
            let d = Arc::new(open(address, fake, events).await.inspect_err(|_| {
                let _ = events.try_send(Event::Disconnected(None));
            })?);
            tokio::spawn(forward(d.clone(), events.clone()));
            let mac = d.mac().await?;
            send(Event::Connected { address: d.address().to_string(), version: d.version(), mac }).await;
            send(Event::Led(d.led(0).await?)).await;
            send(Event::Curve(d.curve().await?)).await;
            send(Event::Shutdown(d.shutdown().await?)).await;
            *dev = Some(d);
        }
        Cmd::Disconnect => {
            if let Some(d) = dev.take() {
                d.disconnect().await;
            }
        }
        Cmd::Mode(mode, preset) => {
            let d = connected(dev)?;
            control::set_mode(d, mode, preset, None).await?;
            // Each mode keeps its own LED effect and the curve belongs to Custom.
            send(Event::Led(d.led(0).await?)).await;
            if mode == Mode::Custom {
                send(Event::Curve(d.curve().await?)).await;
            }
        }
        Cmd::Curve(levels) => control::set_curve(connected(dev)?, &levels).await?,
        Cmd::QuickSwitch(mode) => control::set_quick_switch(connected(dev)?, mode).await?,
        Cmd::Light(on) => control::set_light(connected(dev)?, on).await?,
        Cmd::Overclock(on) => control::set_overclock(connected(dev)?, on, true).await?,
        Cmd::Shutdown(schedule) => {
            let sd = control::set_shutdown(connected(dev)?, schedule).await?;
            send(Event::Shutdown(sd)).await;
        }
        Cmd::Led(code, change) => {
            let d = connected(dev)?;
            let effect = b9x_proto::effect_by_code(code)
                .ok_or_else(|| b9x_ble::Error::Invalid(format!("unknown effect {code:02x}")))?;
            send(Event::Led(control::set_led(d, effect, &change).await?)).await;
        }
        Cmd::RefreshLed => {
            let led = connected(dev)?.led(0).await?;
            send(Event::Led(led)).await;
        }
        Cmd::Quit(_) => unreachable!(),
    }
    Ok(())
}

async fn open(address: Option<String>, fake: bool, events: &Sender<Event>) -> b9x_ble::Result<Device> {
    if fake {
        return Device::open(fake::connect(Duration::from_secs(1)), false).await;
    }
    let address = match address {
        Some(a) => a,
        None => {
            let _ = events.send(Event::Connecting("Looking for a B9X…".into())).await;
            b9x_ble::find(SCAN_TIMEOUT).await?
        }
    };
    let _ = events.send(Event::Connecting(format!("Connecting to {address}…"))).await;
    Device::open(b9x_ble::connect(&address, WriteMode::Request, SCAN_TIMEOUT).await?, false).await
}

/// Pass status pushes and the end of the link on to the UI.
async fn forward(dev: Arc<Device>, events: Sender<Event>) {
    let mut status = dev.watch_status();
    let mut state = dev.watch_state();
    drop(dev);
    loop {
        tokio::select! {
            r = status.changed() => {
                if r.is_err() {
                    break;
                }
                let s = status.borrow_and_update().clone();
                if let Some(s) = s {
                    let _ = events.send(Event::Status(s)).await;
                }
            }
            r = state.changed() => {
                let s = state.borrow_and_update().clone();
                if let LinkState::Disconnected(why) = s {
                    let why = (why != "closed").then_some(why);
                    let _ = events.send(Event::Disconnected(why)).await;
                    break;
                }
                if r.is_err() {
                    break;
                }
            }
        }
    }
}

pub async fn quit_on_signal(events: Sender<Event>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut int), Ok(mut term), Ok(mut hup)) =
            (signal(SignalKind::interrupt()), signal(SignalKind::terminate()), signal(SignalKind::hangup()))
        else {
            return;
        };
        loop {
            tokio::select! {
                _ = int.recv() => {}
                _ = term.recv() => {}
                _ = hup.recv() => {}
            }
            let _ = events.send(Event::Quit).await;
        }
    }
}
