"""Command line control for the Flydigi B9X phone cooler over BLE.

Deprecated: superseded by the Rust workspace in crates/ (b9x-proto, b9x-ble,
b9x-cli, b9x-gui). Kept for reference and for macOS/Windows until those get a
Rust backend; new work goes to the Rust code.
"""
import argparse
import asyncio
import json
import os
import re
import sys
import time

from . import protocol as p

NAME_HINT = 'B9X'
REPLY_TIMEOUT = 3.0


class DeviceError(Exception):
    pass


class B9X:
    def __init__(self, address, debug=False, write_response=True):
        self.address = address
        self.debug = debug
        self.write_response = write_response
        self.queue = asyncio.Queue()
        self.version = None
        self.client = None

    async def __aenter__(self):
        from bleak import BleakClient
        self.client = BleakClient(self.address)
        await self.client.connect()
        await self.client.start_notify(p.NOTIFY_UUID, self._on_notify)
        # The device pushes its firmware version as ASCII right after subscribing.
        await self.request(p.HANDSHAKE, lambda f: f[:2] == p.HANDSHAKE[:2])
        return self

    async def __aexit__(self, *exc):
        await self.client.disconnect()

    def _on_notify(self, _char, data):
        data = bytes(data)
        if self.debug:
            print('<- %s' % data.hex(' '), file=sys.stderr)
        if data[:1].isdigit() and self.version is None:
            self.version = data.split(b'\0')[0].decode('ascii', 'replace')
        self.queue.put_nowait(data)

    async def write(self, frame):
        p.check_frame(frame)
        if self.debug:
            print('-> %s' % frame.hex(' '), file=sys.stderr)
        await self.client.write_gatt_char(p.WRITE_UUID, frame, response=self.write_response)

    async def expect(self, match, timeout=REPLY_TIMEOUT):
        deadline = time.monotonic() + timeout
        while True:
            left = deadline - time.monotonic()
            if left <= 0:
                raise DeviceError('no reply from the device')
            try:
                f = await asyncio.wait_for(self.queue.get(), left)
            except asyncio.TimeoutError:
                raise DeviceError('no reply from the device') from None
            if match(f):
                return f

    def drain(self):
        while not self.queue.empty():
            self.queue.get_nowait()

    async def request(self, frame, match):
        self.drain()
        await self.write(frame)
        return await self.expect(match)

    async def status(self):
        # Status frames arrive about once a second without asking.
        self.drain()
        return p.parse_status(await self.expect(lambda f: f[:1] == b'\x13'))

    async def curve(self):
        return p.parse_curve(await self.request(p.QUERY_CURVE, lambda f: f[:2] == p.QUERY_CURVE))

    async def mac(self):
        return p.parse_mac(await self.request(p.QUERY_MAC, lambda f: f[:1] == b'\x24'))

    async def shutdown(self):
        return p.parse_shutdown(await self.request(p.QUERY_SHUTDOWN, lambda f: f[:1] == b'\x61'))

    async def led(self, code=0):
        def match(f):
            return f[:1] == b'\x51' and (code == 0 or f[1] == code)
        first = await self.request(p.led_query(code), match)
        frames = [first]
        if p.led_frames_needed(first) > 1:
            frames.append(await self.expect(lambda f: match(f) and f[1] == first[1] and f[4] == 2))
        return p.parse_led_reply(frames)


# --- output ------------------------------------------------------------------

def hexcolor(c):
    return '%02x%02x%02x' % c


def effect_label(code):
    e = p.EFFECT_BY_CODE.get(code)
    return '%s (%s)' % (e.name, e.label) if e else 'unknown %02x' % code


def status_dict(s):
    return {
        'mode': p.MODE_NAMES.get(s.mode, s.mode),
        'preset': s.preset_name(),
        'cold_setpoint': s.cold_setpoint,
        'fan_setpoint': s.fan_setpoint,
        'cold_temp': s.cold_temp,
        'hot_temp': s.hot_temp,
        'output': s.output,
        'fan_rpm': s.fan_rpm,
        'light': s.light,
        'effect': getattr(p.EFFECT_BY_CODE.get(s.effect), 'name', s.effect),
        'quick_switch': p.MODE_NAMES.get(s.quick_switch, s.quick_switch),
        'overclock': s.overclock,
        'shutdown_left': s.shutdown_left,
    }


def print_status(s):
    d = status_dict(s)
    mode = d['mode'] + (' / %s' % d['preset'] if d['preset'] else '')
    print('mode          %s' % mode)
    print('setpoint      cold %d, fan %d rpm' % (s.cold_setpoint, s.fan_setpoint))
    print('cold plate    %.1f °C' % s.cold_temp)
    print('hot side      %d °C (estimated)' % s.hot_temp)
    print('output        %d' % s.output)
    print('fan           %d rpm' % s.fan_rpm)
    print('light         %s, effect %s' % ('on' if s.light else 'off', effect_label(s.effect)))
    print('quick switch  %s' % d['quick_switch'])
    print('OC            %s' % ('on' if s.overclock else 'off'))
    if s.shutdown_left:
        print('shutdown in   %dh%02dm' % divmod(s.shutdown_left, 60))


def print_led(s):
    print('effect      %s' % effect_label(s.effect))
    e = p.EFFECT_BY_CODE.get(s.effect)
    if e and e.speeds:
        print('speed       %s (%d)' % (s.speed_name() or '?', s.speed))
    if e and (e.colors or e.fixed_colors):
        print('colors      %s' % ' '.join(hexcolor(c) for c in s.colors))
    print('brightness  %d' % s.brightness)
    if e and e.name == 'rhythm':
        print('rhythm      %s' % ('colorful' if s.color_mode == p.RHYTHM_COLORFUL else 'single'))


def print_shutdown(repeat, enable, minutes):
    if not enable:
        print('scheduled shutdown off')
    else:
        print('scheduled shutdown in %dh%02dm (%s)' % (minutes // 60, minutes % 60,
                                                      'repeat' if repeat else 'once'))


# --- argument types ----------------------------------------------------------

def minutes_arg(text):
    m = re.fullmatch(r'(?:(\d+)h)?(?:(\d+)m?)?', text)
    if not m or not any(m.groups()):
        raise argparse.ArgumentTypeError('use minutes or e.g. 1h30m')
    total = int(m.group(1) or 0) * 60 + int(m.group(2) or 0)
    if not 1 <= total <= p.SHUTDOWN_MAX_MIN:
        raise argparse.ArgumentTypeError('1-%d minutes' % p.SHUTDOWN_MAX_MIN)
    return total


def curve_arg(text):
    try:
        levels = [int(v) for v in text.split(',')]
        p.custom_curve(levels)
    except ValueError as e:
        raise argparse.ArgumentTypeError(str(e)) from None
    return levels


def color_arg(text):
    text = text.lstrip('#')
    if not re.fullmatch(r'[0-9a-fA-F]{6}', text):
        raise argparse.ArgumentTypeError('colour must be RRGGBB')
    return tuple(bytes.fromhex(text))


def brightness_arg(text):
    v = int(text)
    if not p.BRIGHTNESS_MIN <= v <= p.BRIGHTNESS_MAX:
        raise argparse.ArgumentTypeError('%d-%d' % (p.BRIGHTNESS_MIN, p.BRIGHTNESS_MAX))
    return v


# --- commands ----------------------------------------------------------------

async def find_address(args):
    if args.address:
        return args.address
    from bleak import BleakScanner
    dev = await BleakScanner.find_device_by_filter(
        lambda d, ad: NAME_HINT in (d.name or ad.local_name or ''), timeout=args.scan_timeout)
    if dev is None:
        raise DeviceError('no B9X found; is it on and not connected to the phone app?')
    return dev.address


async def cmd_scan(args):
    from bleak import BleakScanner
    found = await BleakScanner.discover(timeout=args.scan_timeout, return_adv=True)
    for d, ad in found.values():
        name = d.name or ad.local_name or ''
        if NAME_HINT in name or p.SERVICE_UUID in ad.service_uuids:
            print('%s  %-16s rssi %d' % (d.address, name, ad.rssi))


async def cmd_info(dev, args):
    print('firmware  %s' % (dev.version or '?'))
    print('mac       %s' % await dev.mac())


async def cmd_status(dev, args):
    while True:
        s = await dev.status()
        if args.json:
            print(json.dumps(status_dict(s)), flush=True)
        else:
            print_status(s)
        if not args.watch:
            return
        if not args.json:
            print()


async def cmd_mode(dev, args):
    if args.mode == 'smart' and not args.preset:
        raise DeviceError('smart needs --preset max or silent')
    if args.preset and (args.mode, args.preset) not in p.PRESETS:
        raise DeviceError('%s has no preset %s' % (args.mode, args.preset))
    if args.curve and args.mode != 'custom':
        raise DeviceError('--curve is for custom mode')
    before = await dev.status()
    if args.curve:
        frame = p.custom_curve(args.curve)
    elif args.preset:
        frame = p.mode_preset(args.mode, args.preset)
    else:
        frame = p.mode_tab(args.mode)
    await dev.write(frame)
    # Like the app: the long-press target becomes the mode we just left.
    if p.MODE_BY_NAME[args.mode] != before.mode and before.mode in p.MODE_NAMES:
        await dev.write(p.quick_switch(p.MODE_NAMES[before.mode]))
    await asyncio.sleep(1.5)
    print_status(await dev.status())


async def cmd_quick_switch(dev, args):
    s = await dev.status()
    if not args.mode:
        print(p.MODE_NAMES.get(s.quick_switch, s.quick_switch))
        return
    if p.MODE_BY_NAME[args.mode] == s.mode:
        raise DeviceError('quick switch cannot be the current mode (%s)' % args.mode)
    if p.MODE_BY_NAME[args.mode] != s.quick_switch:
        await dev.write(p.quick_switch(args.mode))


async def cmd_curve(dev, args):
    levels = await dev.curve()
    for t, v in zip(p.CURVE_TEMPS, levels):
        print('%4d °C  %d %s' % (t, v, '#' * v))


async def cmd_light(dev, args):
    if args.state is None:
        print('on' if (await dev.status()).light else 'off')
        return
    await dev.write(p.light(args.state == 'on'))


async def cmd_oc(dev, args):
    if args.state is None:
        print('on' if (await dev.status()).overclock else 'off')
        return
    if args.state == 'on' and not args.yes:
        raise DeviceError('OC runs at full power with frost protection off; '
                          'condensation can form. Add --yes to turn it on.')
    await dev.write(p.overclock(args.state == 'on'))


async def cmd_shutdown(dev, args):
    if args.minutes is None and not args.off:
        print_shutdown(*await dev.shutdown())
        return
    if args.off:
        # Keep the last duration like the app. The only off frame captured has
        # repeat set, so send that one regardless of the current flag.
        _, enable, minutes = await dev.shutdown()
        if enable:
            await dev.write(p.shutdown(True, False, minutes))
    else:
        await dev.write(p.shutdown(args.repeat, True, args.minutes))
    print_shutdown(*await dev.shutdown())


async def cmd_led(dev, args):
    if args.effect is None:
        print_led(await dev.led())
        return
    e = p.EFFECT_BY_NAME[args.effect]
    changes = args.speed or args.colors or args.brightness or args.rhythm
    if changes and not e.writable:
        raise DeviceError('%s has no settings; select it without options' % e.name)
    if args.speed and not e.speeds:
        raise DeviceError('%s has no speed' % e.name)
    if args.colors and not e.colors:
        raise DeviceError('%s colours cannot be changed' % e.name)
    if args.colors and not e.colors[0] <= len(args.colors) <= e.colors[1]:
        raise DeviceError('%s takes %d-%d colours' % ((e.name,) + e.colors))
    if args.rhythm and e.name != 'rhythm':
        raise DeviceError('--rhythm is for the rhythm effect')

    await dev.write(p.led_select(e.code))
    if changes:
        s = await dev.led(e.code)
        if args.speed:
            s.speed = e.speeds[p.SPEED_NAMES.index(args.speed)]
        elif not e.speeds:
            s.speed = 0
        if args.colors:
            s.colors = list(args.colors)
        if args.brightness:
            s.brightness = args.brightness
        if e.name == 'rhythm':
            if args.rhythm:
                s.color_mode = p.RHYTHM_COLORFUL if args.rhythm == 'colorful' else p.RHYTHM_SINGLE
            elif s.color_mode not in (p.RHYTHM_SINGLE, p.RHYTHM_COLORFUL):
                s.color_mode = p.RHYTHM_SINGLE
        else:
            s.color_mode = 0
        frames = p.led_frames(s)
        for f in frames:
            p.check_frame(f)
        for f in frames:
            await dev.write(f)
    print_led(await dev.led(e.code))


COMMANDS = {
    'info': cmd_info, 'status': cmd_status, 'mode': cmd_mode, 'quick-switch': cmd_quick_switch,
    'curve': cmd_curve, 'light': cmd_light, 'oc': cmd_oc, 'shutdown': cmd_shutdown, 'led': cmd_led,
}


def build_parser():
    ap = argparse.ArgumentParser(prog='b9x', description=__doc__)
    ap.add_argument('-a', '--address', default=os.environ.get('B9X_ADDRESS'),
                    help='device address (macOS: CoreBluetooth UUID). Default: $B9X_ADDRESS or scan')
    ap.add_argument('--scan-timeout', type=float, default=10.0)
    ap.add_argument('--write-cmd', action='store_true',
                    help='write without response, as the Android app does')
    ap.add_argument('-d', '--debug', action='store_true', help='print frames on stderr')
    sub = ap.add_subparsers(dest='command', required=True)

    sub.add_parser('scan', help='list nearby B9X coolers')
    sub.add_parser('info', help='firmware version and MAC')

    s = sub.add_parser('status', help='temperatures, fan, mode')
    s.add_argument('-w', '--watch', action='store_true', help='keep printing every second')
    s.add_argument('--json', action='store_true')

    s = sub.add_parser('mode', help='set cooling mode')
    s.add_argument('mode', choices=list(p.MODE_BY_NAME))
    s.add_argument('--preset', choices=sorted({pr for _, pr in p.PRESETS}),
                   help='smart: max, silent. fixed: energy, strong, max')
    s.add_argument('--curve', type=curve_arg,
                   help='custom: 9 levels 1-8 for %s °C, comma separated'
                   % ','.join(map(str, p.CURVE_TEMPS)))

    s = sub.add_parser('quick-switch', help='mode the power button toggles to on long press')
    s.add_argument('mode', nargs='?', choices=list(p.MODE_BY_NAME))

    sub.add_parser('curve', help='show the custom mode curve')

    s = sub.add_parser('light', help='lighting on/off')
    s.add_argument('state', nargs='?', choices=['on', 'off'])

    s = sub.add_parser('oc', help='OC Experience (full power, frost protection off)')
    s.add_argument('state', nargs='?', choices=['on', 'off'])
    s.add_argument('--yes', action='store_true')

    s = sub.add_parser('shutdown', help='scheduled shutdown')
    g = s.add_mutually_exclusive_group()
    g.add_argument('minutes', nargs='?', type=minutes_arg, help='e.g. 90 or 1h30m (max 4h15m)')
    g.add_argument('--off', action='store_true')
    s.add_argument('--repeat', action='store_true', help='repeat every time instead of once')

    s = sub.add_parser('led', help='show or set the LED effect of the current mode')
    s.add_argument('effect', nargs='?', choices=list(p.EFFECT_BY_NAME))
    s.add_argument('--speed', choices=p.SPEED_NAMES)
    s.add_argument('--colors', type=color_arg, nargs='+', metavar='RRGGBB')
    s.add_argument('--brightness', type=brightness_arg,
                   help='%d-%d' % (p.BRIGHTNESS_MIN, p.BRIGHTNESS_MAX))
    s.add_argument('--rhythm', choices=['single', 'colorful'])
    return ap


async def run(args):
    if args.command == 'scan':
        return await cmd_scan(args)
    address = await find_address(args)
    async with B9X(address, args.debug, not args.write_cmd) as dev:
        await COMMANDS[args.command](dev, args)


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        asyncio.run(run(args))
    except (DeviceError, p.UnsafeFrame) as e:
        sys.exit('b9x: %s' % e)
    except KeyboardInterrupt:
        pass


if __name__ == '__main__':
    main()
