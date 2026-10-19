# Capturing the official app

[한국어](capturing.ko.md)

How the protocol in [protocol.md](protocol.md) was recorded, for checking new
commands. Only frames confirmed this way may be added to the whitelist; never guess
or fuzz opcodes. Related Flydigi firmware has unauthenticated destructive opcodes:
BS3 Pro `0xDF` erases the firmware and `0x06` factory-resets
([ElXreno/flydigictl](https://github.com/ElXreno/flydigictl) `docs/FIRMWARE.md` §3.3),
and the B6 SDK has `FF 0A` for a flash reset.

## Setup

The capture device is a rooted (KernelSU) Android tablet; reading the HCI log needs root.

```sh
adb devices -l                    # USB
adb connect <tablet-ip>:5555      # Wi-Fi; a KernelSU boot script pins the port to 5555
```

Enable the full HCI snoop log (it persists across reboots):

```sh
adb shell su -c 'setprop persist.bluetooth.btsnooplogmode full'
adb shell su -c 'settings put secure bluetooth_hci_log 1'
adb shell cmd bluetooth_manager disable
adb shell cmd bluetooth_manager enable       # Bluetooth must restart to apply
adb shell su -c 'ls -la /data/misc/bluetooth/logs/'
```

Without `full`, only `btsnooz_hci.log` is written and payloads are missing.

## Procedure

1. Optionally record logcat; the app logs some BLE details:
   `adb logcat -v threadtime -T 1 > captures/logcat_$(date +%Y%m%d_%H%M%S).txt`
2. Open the app and connect to the B9X.
3. Do **one action at a time, with a pause in between**, and note the time of each.
4. Pull and parse the log:
   ```sh
   tools/pull_snoop.sh [serial]
   python3 -I tools/btsnoop_att.py captures/btsnoop_….log [--addr AA:BB:…] [--all]
   ```
   The default output shows connections, the GATT table, writes and notifications;
   `--all` adds reads, MTU exchange and other ATT traffic.
5. Match action times to writes, compare with the app's screen, and record the result
   in `docs/protocol.md` as confirmed or inferred. Add the frames to `testdata/app_writes.txt`.

The app's BLE code is in `com.flydigi.sdk.bluetooth` (per B6-Studio's analysis) if
static analysis of the APK is needed.

## Related implementations

```sh
git clone https://github.com/shaoyou11/ha-flydigi-b8x refs/ha-flydigi-b8x   # B8X, Home Assistant
git clone https://github.com/pgs666/B6-Studio refs/B6-Studio                 # B6/B6X, APK analysis
git clone https://github.com/ElXreno/flydigictl refs/ElXreno-flydigictl      # BS laptop coolers, firmware
```

The BS series uses HID frames (`cmd len payload`), unlike the B9X, but shares the SoC
family (WCH CH591/592, presumably), the dangerous opcodes and the
`19 3x f0 f1 f2 f3 f4 + MAC` identifier.
