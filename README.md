# flydigi-b9x-linux

Control the **Flydigi B9X** phone cooler over Bluetooth LE without the official app
(飞智游戏厅, `com.game.motionelf`). Command line tool and GTK 4 app, written in Rust.

[한국어](README.ko.md)

> Unofficial and not affiliated with Flydigi. The protocol was worked out from
> captures of the official app. Use at your own risk.

## Features

- Live status: cold plate temperature, fan speed, hot side, cooling output
- Cooling mode (Smart / Fixed / Custom), presets and the Custom cooling curve
- Quick switch (long press target), OC Experience, scheduled shutdown
- Lighting on/off, LED effects, speed, brightness and colours
- GUI with a temperature graph; it picks the cooler for you, or lets you choose when several are nearby

## Safety

Some Flydigi firmware erases itself or resets on unknown opcodes. This project only
sends frames the official app was seen sending, within the values the app used.
Every write passes a whitelist (`crates/b9x-proto`); there is no way to send an
arbitrary frame. Before anything beyond the handshake is written, the device must
advertise as a B9X and report the MAC address that was connected to.

## Platforms

| Platform | Status |
|---|---|
| Linux (BlueZ) | Supported: CLI and GTK 4 / libadwaita app |
| macOS, Windows | Not yet in Rust. The deprecated Python CLI in `b9x/` runs there |

Backends and GUI toolkits are per OS, so each platform gets its own native build.

## Install (Linux)

```sh
sudo apt install cargo libdbus-1-dev libgtk-4-dev libadwaita-1-dev pkg-config
cargo build --release          # target/release/b9x and target/release/b9x-gui
cargo install --path crates/b9x-cli && cargo install --path crates/b9x-gui   # or install into ~/.cargo/bin
```

The B9X exposes a HID service but rejects pairing, and BlueZ drops the connection
when its HID plugin cannot pair. Set this once in `/etc/bluetooth/input.conf`, then
`sudo systemctl restart bluetooth`. Keyboards and mice that are already bonded are not affected.

```ini
[General]
LEAutoSecurity=false
```

See [docs/linux-bluez.md](docs/linux-bluez.md) for the details.

## Usage

Close the Flydigi app on your phone first: the cooler accepts one connection at a time.

```sh
b9x scan                                  # find coolers
export B9X_ADDRESS=9C:7F:64:C5:F4:83      # optional; otherwise b9x scans
b9x status [--watch] [--json]
b9x mode smart --preset silent            # smart: max, silent / fixed: energy, strong, max
b9x mode custom --curve 5,5,5,6,7,8,8,8,8 # levels 1-8 at -12 … 12 °C
b9x quick-switch fixed
b9x led breathing --speed fast --colors ff0000 0000ff --brightness 60
b9x light off
b9x shutdown 1h30m [--repeat]  |  b9x shutdown --off
b9x oc on --yes                           # full power, frost protection off
b9x disconnect                            # drop a link left by a killed b9x
b9x --fake status                         # simulated device, no hardware needed
```

```sh
b9x-gui [-a ADDRESS] [--fake]
install -Dm644 data/io.github.flydigib9x.B9X.desktop ~/.local/share/applications/io.github.flydigib9x.B9X.desktop
```

## Limitations

- Linux does not show the firmware version: BlueZ drops the version push (see the BlueZ notes).
- Smart Control, Pulsating and Cyberpunk effects can only be selected, not configured.
- Scheduled shutdown is limited to 255 minutes.
- `oc on` has not been tested on the device.

## Documentation

- [docs/development.md](docs/development.md): code layout, tests, behaviour rules
- [docs/linux-bluez.md](docs/linux-bluez.md): BlueZ setup and quirks
- [docs/protocol.md](docs/protocol.md): protocol notes, confirmed vs inferred (Korean)
- [docs/capturing.md](docs/capturing.md): capturing the official app's traffic

## Related projects

- [shaoyou11/ha-flydigi-b8x](https://github.com/shaoyou11/ha-flydigi-b8x): B8X, Home Assistant integration
- [pgs666/B6-Studio](https://github.com/pgs666/B6-Studio): B6/B6X Android app and protocol analysis
- [ElXreno/flydigictl](https://github.com/ElXreno/flydigictl): BS series laptop coolers, firmware analysis

## License

No license has been chosen yet.
