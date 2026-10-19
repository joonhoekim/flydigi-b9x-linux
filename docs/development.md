# Development

[한국어](development.ko.md)

## Layout

| Path | Contents |
|---|---|
| `crates/b9x-proto` | Frame codec and whitelist. No I/O. Transports only accept a checked `Frame` |
| `crates/b9x-ble` | `Device` (one connection, status/reply dispatch), `control` (multi-step changes the way the app does them), backends: `bluez` (Linux), `fake` (simulated device) |
| `crates/b9x-cli` | `b9x` |
| `crates/b9x-gui` | `b9x-gui`, GTK 4 / libadwaita. A tokio worker owns the device; the window talks to it over channels |
| `testdata/app_writes.txt` | Every distinct write the official app made. Shared by the Rust and Python tests |
| `docs/protocol.md` | Protocol notes (Korean) |
| `data/` | Desktop entry, AppStream metainfo and icon, installed by `install.sh` |
| `tools/icons/gen.py` | Generates the dot-grid icon and its drafts (`--preview` for a comparison sheet) |
| `tools/` | `btsnoop_att.py` (ATT parser for btsnoop logs), `pull_snoop.sh` |
| `b9x/`, `tests/` | Deprecated Python implementation |
| `captures/`, `refs/` | Local only, not in git: captures and clones of related projects |

A new platform gets its own backend module in `b9x-ble` (with `scan`, `find`,
`connect`, `disconnect`) and its own GUI crate, rather than a cross-platform abstraction.

## Tests

```sh
cargo test
cargo clippy --all-targets
```

- The whitelist is checked against every captured app write and a list of frames it must reject.
- `control` flows run against the fake device, including status pushes that lag behind
  writes like the real device, and handshakes from the wrong device.
- `b9x --fake …` and `b9x-gui --fake` work without hardware.
  `B9X_FAKE_DEVICES=3 b9x-gui --fake` shows the cooler picker.

## Behaviour rules

- Writes go through `Frame::check`. Multi-frame changes (LED settings) check every frame before sending the first.
- Changing mode sets Quick switch to the mode left, as the app does.
- Status pushes show old values for 1–2 s after a write. Decisions right after a write
  (previous mode, Quick switch) use the values just written, for up to 5 s.
- The Custom curve frame (`20 04 …`) also switches to Custom, so it is only sent in Custom mode.
- Smart Control, Pulsating and Cyberpunk have no settings in the app; it only selects them, and so do we.
- The cooler ends OC Experience when the connection drops, so `b9x oc on` stays connected
  until interrupted, then turns OC off. The GUI keeps it on while connected.
- Gradient was only captured with 8 colours; 2–8 are allowed, fewer than 8 is untested.
- Scheduled shutdown minutes go out as one byte (255 max): the byte order of larger
  values is unconfirmed. Switching off a longer duration set by the phone app stores 255.

## Releasing

1. Update the version in `Cargo.toml`, the `<releases>` list in
   `data/io.github.joonhoekim.FlydigiB9X.metainfo.xml` and `CHANGELOG.md`.
2. Commit, tag `vX.Y.Z` and push the tag. `.github/workflows/release.yml` builds
   x86_64 and aarch64 tarballs on Debian 13 and publishes a GitHub release with the
   CHANGELOG section as notes.

## Python (deprecated)

`b9x/` is the earlier CLI on [bleak](https://github.com/hbldh/bleak). It is kept for
reference and for macOS/Windows until those get a Rust backend. Its whitelist must
stay identical to the Rust one; both test against `testdata/app_writes.txt`.

```sh
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/python -m unittest discover -s tests
```
