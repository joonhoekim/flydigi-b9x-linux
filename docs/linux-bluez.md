# Linux (BlueZ) notes

[한국어](linux-bluez.ko.md)

Tested with BlueZ 5.82. The backend is `crates/b9x-ble/src/bluez.rs` (bluer).

## Required setting: `LEAutoSecurity=false`

The B9X advertises a HID service (`1812`) but answers SMP pairing requests with
*Pairing Not Supported*. BlueZ's HID plugin asks for pairing as soon as it connects
to an LE HID device that is not bonded, and disconnects when that fails, about two
seconds after connecting.

```ini
# /etc/bluetooth/input.conf
[General]
LEAutoSecurity=false
```

```sh
sudo systemctl restart bluetooth
```

`profiles/input/hog.c` only reads this option for devices that are not bonded, so
keyboards and mice that are already bonded keep working as before.

## Behaviour to know about

**Firmware version.** The B9X pushes its version (`30 2e 33 2e 30 00`, "0.3.0")
right after notifications are enabled, but before it acknowledges the CCCD write.
bluetoothd drops notifications that arrive while that write is pending, so the
version never reaches us. The Device Information Service only holds placeholder
strings ("Firmware Revision" etc.). `b9x info` prints `?` on Linux.

**Links outlive the process.** If a process dies without disconnecting, BlueZ keeps
the LE link up and the phone app cannot connect. `b9x` and `b9x-gui` disconnect on
Ctrl-C, SIGTERM, SIGHUP and a closed stdout, and when interrupted while connecting.
After `kill -9`, run `b9x disconnect`.

**Connecting.** BlueZ forgets an unpaired device when discovery stops, unless a
Connect call for it has already arrived. The kernel, on the other hand, holds an LE
connect back until the current scan cycle ends, about 10 seconds. So the backend
waits for the advertisement, sends Connect, and stops discovery right after. A connect
can also hang inside BlueZ, so every attempt has a timeout, and Disconnect is sent
when one is abandoned (it also cancels a pending connect).

**Identifying the cooler.** The B9X puts service `ffe0` in its advertisement and the
name `Flydigi B9X` in its scan response. `ffe0` alone is shared with HM-10 style
serial modules, so both must match, also when an address is given with `-a`. A device
that is already connected is checked the same way before it is reused.

**Write type.** The app writes without response, but the characteristic properties
(`0x0a`) do not include that, so the default is write request. `--write-cmd` sends
like the app; both work on BlueZ.
