"""B9X frame codec and the whitelist of frames the official app was seen sending.

Every frame that goes to the device passes through `check_frame()`. It only
accepts the opcodes and value ranges recorded in docs/protocol.md; anything
else raises `UnsafeFrame`. Some Flydigi firmware erases itself on unknown
opcodes, so there is deliberately no way to send an arbitrary frame.
"""
from dataclasses import dataclass, field

SERVICE_UUID = '0000ffe0-0000-1000-8000-00805f9b34fb'
WRITE_UUID = '0000ffe1-0000-1000-8000-00805f9b34fb'
NOTIFY_UUID = '0000ffe4-0000-1000-8000-00805f9b34fb'


class UnsafeFrame(ValueError):
    """The frame is not one the official app sends."""


# --- modes -------------------------------------------------------------------

SMART, FIXED, CUSTOM = 0x00, 0x01, 0x04
MODE_NAMES = {SMART: 'smart', FIXED: 'fixed', CUSTOM: 'custom'}
MODE_BY_NAME = {v: k for k, v in MODE_NAMES.items()}

# (mode, preset) -> (cold, fan rpm, smart sub-preset byte)
PRESETS = {
    ('smart', 'max'): (175, 5500, 0x00),
    ('smart', 'silent'): (145, 4000, 0x06),
    ('fixed', 'energy'): (145, 4000, 0x00),
    ('fixed', 'strong'): (155, 4500, 0x00),
    ('fixed', 'max'): (175, 5700, 0x00),
}

CURVE_TEMPS = (-12, -9, -6, -3, 0, 3, 6, 9, 12)
CURVE_MIN, CURVE_MAX = 1, 8


def _setpoint(cold, fan):
    return bytes([cold, fan // 100, fan % 100])


def mode_preset(mode, preset):
    cold, fan, sub = PRESETS[(mode, preset)]
    sp = _setpoint(cold, fan)
    return bytes([0x20, MODE_BY_NAME[mode]]) + sp + bytes([sub]) + sp


def mode_tab(mode):
    """Switch mode and keep the preset or curve stored for it."""
    if mode == 'fixed':
        return bytes([0x20, FIXED]) + bytes(7)
    if mode == 'custom':
        return bytes([0x20, CUSTOM]) + bytes(9)
    raise ValueError('smart needs a preset')


def custom_curve(levels):
    levels = list(levels)
    if len(levels) != len(CURVE_TEMPS) or not all(CURVE_MIN <= v <= CURVE_MAX for v in levels):
        raise ValueError('curve needs %d levels, each %d-%d' % (len(CURVE_TEMPS), CURVE_MIN, CURVE_MAX))
    return bytes([0x20, CUSTOM] + levels)


def quick_switch(mode):
    return bytes([0x21, MODE_BY_NAME[mode]])


HANDSHAKE = bytes.fromhex('a5a00000')
QUERY_CURVE = bytes.fromhex('2204')
QUERY_MAC = bytes.fromhex('2400')
QUERY_SHUTDOWN = bytes.fromhex('61')


def light(on):
    return bytes([0x10, 0xff if on else 0x00])


def overclock(on):
    return bytes([0x26, 0x01 if on else 0x00, 0x00, 0x00])


# Minutes go out as two bytes. Only values below 256 were captured, so the
# high byte stays 0 until its byte order is confirmed.
SHUTDOWN_MAX_MIN = 255


def shutdown(repeat, enable, minutes):
    if not 0 <= minutes <= SHUTDOWN_MAX_MIN or (enable and minutes == 0):
        raise ValueError('minutes must be 1-%d' % SHUTDOWN_MAX_MIN)
    return bytes([0x60, int(repeat), int(enable), 0x00, minutes])


# --- LED ---------------------------------------------------------------------

@dataclass(frozen=True)
class Effect:
    code: int
    name: str
    label: str
    colors: tuple = None     # (min, max) the user may set; None = not settable
    speeds: tuple = None     # (slow, medium, fast); None = no speed
    writable: bool = True    # False: the app only ever selected it
    fixed_colors: int = 0    # colours the app passes through unchanged


EFFECTS = [
    Effect(0x0c, 'smart', 'Smart Control', writable=False),
    Effect(0x03, 'flow', 'Flow', speeds=(6, 4, 2), fixed_colors=1),
    Effect(0x06, 'gradient', 'Gradient', colors=(2, 8), speeds=(10, 8, 6)),
    Effect(0x05, 'breathing', 'Breathing', colors=(2, 8), speeds=(8, 5, 3)),
    Effect(0x04, 'cutover', 'Cutover', colors=(2, 8), speeds=(8, 5, 3)),
    Effect(0x0b, 'always-on', 'Always on', colors=(1, 1)),
    Effect(0x10, 'rotation', 'Multi-color Rotation', colors=(2, 8), speeds=(8, 5, 3)),
    Effect(0x11, 'pulsating', 'Pulsating', writable=False),
    Effect(0x13, 'rhythm', 'Rhythm', colors=(1, 1)),
    Effect(0x14, 'cyberpunk', 'Cyberpunk', writable=False),
    Effect(0x15, 'collision', 'Collision', colors=(2, 8), speeds=(3, 5, 8)),
]
EFFECT_BY_CODE = {e.code: e for e in EFFECTS}
EFFECT_BY_NAME = {e.name: e for e in EFFECTS}
SPEED_NAMES = ('slow', 'medium', 'fast')
BRIGHTNESS_MIN, BRIGHTNESS_MAX = 20, 100
RHYTHM_SINGLE, RHYTHM_COLORFUL = 0x01, 0x02
COLORS_PER_FRAME = 4


@dataclass
class LedSettings:
    effect: int
    speed: int = 0
    colors: list = field(default_factory=list)   # [(r, g, b), ...]
    brightness: int = 0
    color_mode: int = 0                          # rhythm only

    def speed_name(self):
        e = EFFECT_BY_CODE.get(self.effect)
        if e and e.speeds and self.speed in e.speeds:
            return SPEED_NAMES[e.speeds.index(self.speed)]
        return None


def checksum(frame):
    return sum(frame) & 0xff


def led_select(code):
    """Switch effect; the device applies the settings stored for it."""
    frame = bytes([0x50, code]) + bytes(17)
    return frame + bytes([checksum(frame)])


def led_query(code=0):
    return bytes([0x51, code])


def led_frames(s):
    """Full settings write, split into 4-colour frames like the app does."""
    chunks = [s.colors[i:i + COLORS_PER_FRAME] for i in range(0, len(s.colors), COLORS_PER_FRAME)]
    frames = []
    for pkt, chunk in enumerate(chunks, 1):
        rgb = b''.join(bytes(c) for c in chunk).ljust(12, b'\0')
        frame = bytes([0x50, s.effect, s.speed, len(s.colors), pkt]) + rgb + bytes([s.brightness, s.color_mode])
        frames.append(frame + bytes([checksum(frame)]))
    return frames


def parse_led_reply(frames):
    """Join one or two `51` replies into LedSettings."""
    first = frames[0]
    n = first[3]
    colors = []
    for f in sorted(frames, key=lambda f: f[4]):
        for i in range(COLORS_PER_FRAME):
            if len(colors) < n:
                colors.append(tuple(f[5 + 3 * i:8 + 3 * i]))
    return LedSettings(effect=first[1], speed=first[2], colors=colors,
                       brightness=first[17], color_mode=first[18])


def led_frames_needed(reply):
    return 2 if reply[3] > COLORS_PER_FRAME else 1


# --- status ------------------------------------------------------------------

@dataclass
class Status:
    mode: int
    cold_setpoint: int
    fan_setpoint: int
    smart_preset: int
    cold_temp: float
    hot_temp: int
    output: int
    fan_rpm: int
    light: bool
    effect: int
    quick_switch: int
    overclock: bool

    def preset_name(self):
        mode = MODE_NAMES.get(self.mode)
        for (m, p), (cold, fan, sub) in PRESETS.items():
            if m == mode and (cold, fan) == (self.cold_setpoint, self.fan_setpoint) \
                    and (m != 'smart' or sub == self.smart_preset):
                return p
        if mode == 'smart':
            return {0x00: 'max', 0x06: 'silent'}.get(self.smart_preset)
        return None


def cold_temp(hi, lo):
    t = abs(hi - 256 if hi >= 0x80 else hi) + lo / 10
    return -t if hi >= 0x80 else t


def parse_status(f):
    if len(f) < 18 or f[0] != 0x13:
        raise ValueError('not a status frame')
    return Status(
        mode=f[2] >> 4,
        cold_setpoint=f[3],
        fan_setpoint=f[4] * 100 + f[5],
        smart_preset=f[6],
        cold_temp=round(cold_temp(f[7], f[8]), 1),
        hot_temp=f[9],
        output=f[10],
        fan_rpm=f[12] * 100 + f[13],
        light=f[14] != 0,
        effect=f[15],
        quick_switch=f[16],
        overclock=bool(f[17] & 0x80),
    )


def parse_shutdown(f):
    """`61` reply -> (repeat, enable, minutes)."""
    return bool(f[1]), bool(f[2]), f[3] * 256 + f[4]


def parse_curve(f):
    return list(f[2:2 + len(CURVE_TEMPS)])


def parse_mac(f):
    return ':'.join('%02X' % b for b in f[1:7][::-1])


# --- whitelist ---------------------------------------------------------------

_EXACT = {HANDSHAKE, QUERY_CURVE, QUERY_MAC, QUERY_SHUTDOWN, light(True), light(False),
          overclock(True), overclock(False), mode_tab('fixed'), mode_tab('custom')}
_EXACT |= {mode_preset(m, p) for m, p in PRESETS}
# Tab switches the app sent with the Smart sub-preset byte left over.
_EXACT |= {bytes([0x20, m, 0, 0, 0, x, 0, 0, 0]) for m in (SMART, FIXED) for x in (0x00, 0x06)}
_EXACT |= {quick_switch(m) for m in MODE_BY_NAME}
_EXACT |= {led_query(0)} | {led_query(e.code) for e in EFFECTS}
_EXACT |= {led_select(e.code) for e in EFFECTS}


def _check_led(f):
    if len(f) != 20 or checksum(f[:19]) != f[19]:
        raise UnsafeFrame('bad LED frame length or checksum')
    e = EFFECT_BY_CODE.get(f[1])
    if e is None or not e.writable:
        raise UnsafeFrame('effect %02x only takes a select frame' % f[1])
    speed, n, pkt, bright, cmode = f[2], f[3], f[4], f[17], f[18]
    if speed not in (e.speeds or (0,)):
        raise UnsafeFrame('speed %d not used by the app for %s' % (speed, e.name))
    lo, hi = e.colors or (e.fixed_colors, e.fixed_colors)
    if not lo <= n <= hi:
        raise UnsafeFrame('%d colours not allowed for %s' % (n, e.name))
    frames = -(-n // COLORS_PER_FRAME)
    if not 1 <= pkt <= frames:
        raise UnsafeFrame('packet number %d out of range' % pkt)
    used = min(COLORS_PER_FRAME, n - (pkt - 1) * COLORS_PER_FRAME)
    if any(f[5 + 3 * used:17]):
        raise UnsafeFrame('unused colour slots must be zero')
    if not BRIGHTNESS_MIN <= bright <= BRIGHTNESS_MAX:
        raise UnsafeFrame('brightness %d out of range' % bright)
    allowed = (RHYTHM_SINGLE, RHYTHM_COLORFUL) if e.name == 'rhythm' else (0,)
    if cmode not in allowed:
        raise UnsafeFrame('colour mode %d not used by the app for %s' % (cmode, e.name))


def check_frame(f):
    f = bytes(f)
    if f in _EXACT:
        return
    if len(f) == 11 and f[:2] == bytes([0x20, CUSTOM]):
        if all(CURVE_MIN <= v <= CURVE_MAX for v in f[2:]):
            return
        raise UnsafeFrame('curve levels out of range: %s' % f.hex(' '))
    if len(f) == 5 and f[0] == 0x60:
        if f[1] in (0, 1) and f[2] in (0, 1) and f[3] == 0 and (f[4] or not f[2]):
            return
        raise UnsafeFrame('shutdown frame out of range: %s' % f.hex(' '))
    if f[:1] == b'\x50':
        return _check_led(f)
    raise UnsafeFrame('frame not on the whitelist: %s' % f.hex(' '))
