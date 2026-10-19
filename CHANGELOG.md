# Changelog

## 0.2.0 - 2026-10-19

First release of the Rust implementation.

- `b9x` CLI with the same commands as the earlier Python CLI, plus `disconnect` and `--fake`.
- `b9x-gui`, a GTK 4 / libadwaita app: live status with a temperature graph, cooling
  mode, presets and curve, Quick switch, OC Experience, lighting and LED effects,
  scheduled shutdown, and a picker when several coolers are nearby.
- Linux backend on BlueZ, tested with BlueZ 5.82. Needs `LEAutoSecurity=false`
  (see docs/linux-bluez.md).
- Every write passes the whitelist of frames captured from the official app. The
  cooler must advertise as a B9X and report the connected MAC address before anything
  beyond the handshake is written.
- `oc on` keeps the connection open: the cooler ends OC when the connection drops.
- The Python CLI in `b9x/` is deprecated.
