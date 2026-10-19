//! Linux backend on BlueZ through bluer.

use std::collections::HashMap;
use std::time::Duration;

use b9x_proto as p;
use bluer::gatt::WriteOp;
use bluer::gatt::remote::{Characteristic, CharacteristicWriteRequest};
use bluer::{Adapter, AdapterEvent, Address, DeviceEvent, DeviceProperty, Uuid};
use futures::{StreamExt, pin_mut};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, timeout, timeout_at};

use crate::{Error, Found, Link, LinkEvent, LinkRequest, NAME_HINT, Result, WriteMode};

const CONNECT_TRIES: u32 = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Time for bluetoothd to take the Connect call before discovery stops.
const CONNECT_HANDOFF: Duration = Duration::from_millis(300);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

impl From<bluer::Error> for Error {
    fn from(e: bluer::Error) -> Self {
        Error::Ble(e.to_string())
    }
}

async fn adapter() -> Result<Adapter> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    if !adapter.is_powered().await? {
        return Err(Error::Ble(format!("adapter {} is powered off", adapter.name())));
    }
    Ok(adapter)
}

async fn describe(adapter: &Adapter, addr: Address) -> Result<Found> {
    let dev = adapter.device(addr)?;
    Ok(Found {
        address: addr.to_string(),
        name: dev.name().await?.or(dev.alias().await.ok()).unwrap_or_default(),
        rssi: dev.rssi().await?,
    })
}

async fn is_b9x(adapter: &Adapter, addr: Address) -> bool {
    let Ok(dev) = adapter.device(addr) else { return false };
    let name = dev.name().await.ok().flatten().unwrap_or_default();
    let service = dev.uuids().await.ok().flatten().is_some_and(|u| u.contains(&Uuid::from_u128(p::SERVICE_UUID)));
    name.contains(NAME_HINT) || service
}

async fn set_le_filter(adapter: &Adapter) -> Result<()> {
    let filter = bluer::DiscoveryFilter { transport: bluer::DiscoveryTransport::Le, ..Default::default() };
    Ok(adapter.set_discovery_filter(filter).await?)
}

/// Run LE discovery until `timeout`, or until `stop` accepts a device.
async fn discover(adapter: &Adapter, timeout: Duration, mut stop: impl FnMut(&Found) -> bool) -> Result<Vec<Found>> {
    set_le_filter(adapter).await?;
    let events = adapter.discover_devices_with_changes().await?;
    pin_mut!(events);
    let deadline = Instant::now() + timeout;
    let mut seen: HashMap<Address, Found> = HashMap::new();
    while let Ok(Some(ev)) = timeout_at(deadline, events.next()).await {
        let AdapterEvent::DeviceAdded(addr) = ev else { continue };
        if !is_b9x(adapter, addr).await {
            continue;
        }
        let found = describe(adapter, addr).await?;
        // Cached devices are listed too; only count ones advertising now.
        if found.rssi.is_none() {
            continue;
        }
        let done = stop(&found);
        seen.insert(addr, found);
        if done {
            break;
        }
    }
    let mut found: Vec<Found> = seen.into_values().collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.rssi));
    Ok(found)
}

/// B9X coolers seen within `timeout`, strongest signal first.
pub async fn scan(timeout: Duration) -> Result<Vec<Found>> {
    discover(&adapter().await?, timeout, |_| false).await
}

/// Address of the first B9X that shows up.
pub async fn find(timeout: Duration) -> Result<String> {
    let found = discover(&adapter().await?, timeout, |_| true).await?;
    found
        .into_iter()
        .next()
        .map(|f| f.address)
        .ok_or_else(|| Error::NotFound("no B9X found; is it on and not connected to the phone app?".into()))
}

async fn characteristic(dev: &bluer::Device, uuid: u128) -> Result<Characteristic> {
    let want = Uuid::from_u128(uuid);
    for service in dev.services().await? {
        if service.uuid().await? != Uuid::from_u128(p::SERVICE_UUID) {
            continue;
        }
        for ch in service.characteristics().await? {
            if ch.uuid().await? == want {
                return Ok(ch);
            }
        }
    }
    Err(Error::NotFound(format!("characteristic {want} not found")))
}

fn flatten(r: Result<bluer::Result<()>, tokio::time::error::Elapsed>) -> Result<(), String> {
    match r {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => Err("timed out".into()),
    }
}

/// Scan until `addr` advertises, then connect.
///
/// BlueZ forgets an unpaired device when discovery stops, unless a Connect call has
/// already arrived for it. But the kernel holds an LE connect until the current scan
/// cycle ends (about 10 s). So: wait for the advertisement, send Connect, and only
/// then stop discovery so the connect goes out at once.
async fn connect_device(adapter: &Adapter, addr: Address, scan_timeout: Duration) -> Result<bluer::Device> {
    let dev = adapter.device(addr)?;
    if dev.is_connected().await.unwrap_or(false) {
        return Ok(dev);
    }
    set_le_filter(adapter).await?;
    let mut events = Box::pin(adapter.discover_devices_with_changes().await?);
    let deadline = Instant::now() + scan_timeout;
    while dev.rssi().await.ok().flatten().is_none() {
        if !matches!(timeout_at(deadline, events.next()).await, Ok(Some(_))) {
            return Err(Error::NotFound(format!("{addr} not found; is it on and not connected to the phone app?")));
        }
    }
    let mut result = {
        let mut connect = Box::pin(dev.connect());
        let early = tokio::select! {
            r = &mut connect => Some(r),
            _ = sleep(CONNECT_HANDOFF) => None,
        };
        drop(events);
        let r = match early {
            Some(r) => Ok(r),
            None => timeout(CONNECT_TIMEOUT, connect).await,
        };
        flatten(r)
    };
    // BlueZ often fails the first LE connect with le-connection-abort-by-local.
    for _ in 1..CONNECT_TRIES {
        if result.is_ok() || dev.is_connected().await.unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(500)).await;
        result = flatten(timeout(CONNECT_TIMEOUT, dev.connect()).await);
    }
    if !dev.is_connected().await.unwrap_or(false) {
        // Also cancels a connect that timed out on our side but is still pending in BlueZ.
        let _ = dev.disconnect().await;
        let why = result.err().unwrap_or_default();
        return Err(Error::Ble(format!("connect {addr}: {why}")));
    }
    Ok(dev)
}

/// Connect, subscribe to notifications and hand the connection over as a [`Link`].
pub async fn connect(address: &str, mode: WriteMode, scan_timeout: Duration) -> Result<Link> {
    let addr: Address = address.parse().map_err(|_| Error::Invalid(format!("bad address {address}")))?;
    let adapter = adapter().await?;
    let dev = connect_device(&adapter, addr, scan_timeout).await?;
    // Anything failing from here on must not leave BlueZ holding the link.
    let setup = async {
        let resolve_deadline = Instant::now() + RESOLVE_TIMEOUT;
        while !dev.is_services_resolved().await? {
            if Instant::now() > resolve_deadline {
                return Err(Error::Ble("GATT services not resolved".into()));
            }
            sleep(Duration::from_millis(100)).await;
        }
        let write = characteristic(&dev, p::WRITE_UUID).await?;
        let notify = characteristic(&dev, p::NOTIFY_UUID).await?;
        let notes = notify.notify().await?;
        let dev_events = dev.events().await?;
        Ok::<_, Error>((write, notes, dev_events))
    };
    let (write, notes, dev_events) = match setup.await {
        Ok(v) => v,
        Err(e) => {
            let _ = dev.disconnect().await;
            return Err(e);
        }
    };

    let (req_tx, mut req_rx) = mpsc::channel::<LinkRequest>(8);
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let op_type = match mode {
        WriteMode::Request => WriteOp::Request,
        WriteMode::Command => WriteOp::Command,
    };
    tokio::spawn(async move {
        pin_mut!(notes);
        pin_mut!(dev_events);
        let why = loop {
            tokio::select! {
                n = notes.next() => match n {
                    Some(data) => { let _ = ev_tx.send(LinkEvent::Notify(data)); }
                    None => break "notification stream ended".to_string(),
                },
                ev = dev_events.next() => match ev {
                    Some(DeviceEvent::PropertyChanged(DeviceProperty::Connected(false))) =>
                        break "device disconnected".to_string(),
                    Some(_) => {}
                    None => break "device removed".to_string(),
                },
                r = req_rx.recv() => match r {
                    Some(LinkRequest::Write(frame, done)) => {
                        let req = CharacteristicWriteRequest { op_type, ..Default::default() };
                        let res = write.write_ext(frame.as_bytes(), &req).await.map_err(Error::from);
                        let _ = done.send(res);
                    }
                    Some(LinkRequest::Close(done)) => {
                        let _ = dev.disconnect().await;
                        let _ = ev_tx.send(LinkEvent::Disconnected("closed".into()));
                        let _ = done.send(());
                        return;
                    }
                    None => {
                        let _ = dev.disconnect().await;
                        return;
                    }
                },
            }
        };
        let _ = ev_tx.send(LinkEvent::Disconnected(why));
    });
    Ok(Link { address: addr.to_string(), requests: req_tx, events: ev_rx })
}

/// Drop BlueZ connections to B9X coolers, or only to `address`. BlueZ keeps a link up
/// after the process that opened it dies. Returns the addresses disconnected.
///
/// With an address, Disconnect goes out even when BlueZ does not report a connection
/// yet: it also cancels a connect still pending from a process that gave up on it.
pub async fn disconnect(address: Option<&str>) -> Result<Vec<String>> {
    let adapter = adapter().await?;
    let mut done = Vec::new();
    for addr in adapter.device_addresses().await? {
        let dev = adapter.device(addr)?;
        let connected = dev.is_connected().await.unwrap_or(false);
        match address {
            Some(a) if a.eq_ignore_ascii_case(&addr.to_string()) => {
                // Fails with "Not Connected" when there was nothing to drop.
                if dev.disconnect().await.is_ok() && connected {
                    done.push(addr.to_string());
                }
            }
            None if connected && is_b9x(&adapter, addr).await => {
                dev.disconnect().await?;
                done.push(addr.to_string());
            }
            _ => {}
        }
    }
    Ok(done)
}
