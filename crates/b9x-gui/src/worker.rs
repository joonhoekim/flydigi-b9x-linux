//! The tokio side: owns the connection and runs one command at a time.
//!
//! The GTK main loop sends [`Cmd`]s and receives [`Event`]s over async channels, so
//! no BLE work ever blocks the UI.

use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use b9x_ble::control::{self, LedChange};
use b9x_ble::{Device, Found, LinkState, WriteMode, fake};
use b9x_proto::{LedSettings, Mode, Preset, ShutdownInfo, Status};
use tokio::sync::oneshot;

pub const SCAN_TIMEOUT: Duration = Duration::from_secs(10);
/// After the first B9X shows up, keep scanning this long for others to choose from.
pub const SCAN_SETTLE: Duration = Duration::from_millis(2500);

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
        /// The handshake had an unexpected product id.
        warning: Option<String>,
    },
    /// None when the user asked for it.
    Disconnected(Option<String>),
    Status(Status),
    Led(LedSettings),
    Curve(Vec<u8>),
    Shutdown(ShutdownInfo),
    Error(String),
    /// More than one B9X answered the scan; the user picks one.
    Choose(Vec<Found>),
    /// SIGINT, SIGTERM or SIGHUP arrived.
    Quit,
}

/// What a connection attempt ended with.
enum Opened {
    Device(Arc<Device>),
    Choose(Vec<Found>),
}

/// What ended a connection attempt.
enum Attempt {
    Done(b9x_ble::Result<Opened>),
    Cancelled(Option<oneshot::Sender<()>>),
}

/// `fake_devices` is how many simulated coolers a scan finds, to try the picker.
pub async fn run(cmds: Receiver<Cmd>, events: Sender<Event>, fake_devices: usize) {
    let mut dev: Option<Arc<Device>> = None;
    while let Ok(cmd) = cmds.recv().await {
        if let Cmd::Connect { address, fake } = cmd {
            if let Some(d) = dev.take() {
                d.disconnect().await;
            }
            // Scanning and connecting can take half a minute; Disconnect and Quit
            // must not wait behind it.
            let attempt = {
                let mut connecting = std::pin::pin!(connect(address.clone(), fake, fake_devices, &events));
                loop {
                    tokio::select! {
                        r = &mut connecting => break Attempt::Done(r),
                        c = cmds.recv() => match c {
                            Ok(Cmd::Quit(done)) => break Attempt::Cancelled(Some(done)),
                            Ok(Cmd::Disconnect) | Err(_) => break Attempt::Cancelled(None),
                            // The controls are hidden while connecting.
                            Ok(_) => {}
                        },
                    }
                }
            };
            match attempt {
                Attempt::Done(Ok(Opened::Device(d))) => {
                    tokio::spawn(forward(d.clone(), events.clone()));
                    dev = Some(d);
                }
                Attempt::Done(Ok(Opened::Choose(found))) => {
                    let _ = events.send(Event::Choose(found)).await;
                }
                Attempt::Done(Err(e)) => {
                    let _ = events.send(Event::Disconnected(None)).await;
                    let _ = events.send(Event::Error(e.to_string())).await;
                }
                Attempt::Cancelled(quit) => {
                    // BlueZ may have finished the connect we just dropped.
                    if !fake {
                        let _ = b9x_ble::disconnect(address.as_deref()).await;
                    }
                    let _ = events.send(Event::Disconnected(None)).await;
                    if let Some(done) = quit {
                        let _ = done.send(());
                        return;
                    }
                }
            }
            continue;
        }
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
        Cmd::Connect { .. } | Cmd::Quit(_) => unreachable!(),
    }
    Ok(())
}

/// Open the device and read what the window shows. Dropping this future before it
/// finishes drops the link, which closes it.
async fn connect(
    address: Option<String>,
    fake: bool,
    fake_devices: usize,
    events: &Sender<Event>,
) -> b9x_ble::Result<Opened> {
    let address = match (address, fake) {
        (Some(a), _) => Some(a),
        (None, true) if fake_devices > 1 => {
            let found = (0..fake_devices)
                .map(|i| Found {
                    address: format!("00:00:00:00:B9:{:02X}", 0x0f + i),
                    name: "fake B9X".into(),
                    rssi: Some(-50 - 5 * i as i16),
                })
                .collect();
            return Ok(Opened::Choose(found));
        }
        (None, true) => None,
        (None, false) => {
            let _ = events.send(Event::Connecting("Looking for a B9X…".into())).await;
            let mut found = b9x_ble::find_all(SCAN_TIMEOUT, SCAN_SETTLE).await?;
            match found.len() {
                0 => return Err(b9x_ble::not_found()),
                1 => Some(found.remove(0).address),
                _ => return Ok(Opened::Choose(found)),
            }
        }
    };
    let d = Arc::new(open(address, fake, events).await?);
    let mac = d.mac().await?;
    let send = |e: Event| async move {
        let _ = events.send(e).await;
    };
    let warning = d.identity_warning().map(str::to_string);
    send(Event::Connected { address: d.address().to_string(), version: d.version(), mac, warning }).await;
    send(Event::Led(d.led(0).await?)).await;
    send(Event::Curve(d.curve().await?)).await;
    send(Event::Shutdown(d.shutdown().await?)).await;
    Ok(Opened::Device(d))
}

/// The simulated device always answers as [`fake::ADDRESS`], whichever was picked.
async fn open(address: Option<String>, fake: bool, events: &Sender<Event>) -> b9x_ble::Result<Device> {
    if fake {
        return Device::open(fake::connect(Duration::from_secs(1)), false).await;
    }
    let address = address.ok_or_else(b9x_ble::not_found)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    fn spawn() -> (Sender<Cmd>, Receiver<Event>, tokio::task::JoinHandle<()>) {
        let (cmd_tx, cmd_rx) = async_channel::unbounded();
        let (ev_tx, ev_rx) = async_channel::unbounded();
        (cmd_tx, ev_rx, tokio::spawn(run(cmd_rx, ev_tx, 1)))
    }

    async fn next_matching(events: &Receiver<Event>, f: impl Fn(&Event) -> bool) -> Event {
        timeout(Duration::from_secs(5), async {
            loop {
                let e = events.recv().await.unwrap();
                if f(&e) {
                    return e;
                }
            }
        })
        .await
        .expect("event did not arrive")
    }

    #[tokio::test]
    async fn quit_does_not_wait_behind_connect() {
        let (tx, _events, worker) = spawn();
        tx.send(Cmd::Connect { address: None, fake: true }).await.unwrap();
        let (done, done_rx) = oneshot::channel();
        tx.send(Cmd::Quit(done)).await.unwrap();
        timeout(Duration::from_secs(2), done_rx).await.expect("quit timed out").unwrap();
        timeout(Duration::from_secs(2), worker).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn disconnect_cancels_connect_and_worker_stays_usable() {
        let (tx, events, _worker) = spawn();
        tx.send(Cmd::Connect { address: None, fake: true }).await.unwrap();
        tx.send(Cmd::Disconnect).await.unwrap();
        next_matching(&events, |e| matches!(e, Event::Disconnected(None))).await;

        tx.send(Cmd::Connect { address: None, fake: true }).await.unwrap();
        next_matching(&events, |e| matches!(e, Event::Shutdown(_))).await;
        tx.send(Cmd::Mode(Mode::Fixed, Some(Preset::Max))).await.unwrap();
        // Fixed keeps its own effect, Cutover on the simulated device.
        let led = next_matching(&events, |e| matches!(e, Event::Led(_))).await;
        assert!(matches!(led, Event::Led(l) if l.effect == 0x04));
        // The curve is refused outside Custom instead of switching back to it.
        tx.send(Cmd::Curve(vec![5; 9])).await.unwrap();
        next_matching(&events, |e| matches!(e, Event::Error(_))).await;
    }

    #[tokio::test]
    async fn several_coolers_offer_a_choice() {
        let (cmd_tx, cmd_rx) = async_channel::unbounded();
        let (ev_tx, events) = async_channel::unbounded();
        tokio::spawn(run(cmd_rx, ev_tx, 2));
        cmd_tx.send(Cmd::Connect { address: None, fake: true }).await.unwrap();
        let e = next_matching(&events, |e| matches!(e, Event::Choose(_) | Event::Connected { .. })).await;
        let Event::Choose(found) = e else { panic!("connected without asking") };
        assert_eq!(found.len(), 2);
        // Picking one connects to it.
        cmd_tx.send(Cmd::Connect { address: Some(found[1].address.clone()), fake: true }).await.unwrap();
        next_matching(&events, |e| matches!(e, Event::Connected { .. })).await;
    }
}
