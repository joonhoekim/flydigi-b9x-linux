"""Codec and whitelist checks against frames captured from the official app."""
import unittest

from b9x import protocol as p

# Every distinct write the app made in captures 1-4 (docs/protocol.md).
APP_WRITES = '''
10 00
10 ff
20 00 00 00 00 00 00 00 00
20 00 00 00 00 06 00 00 00
20 00 91 28 00 06 91 28 00
20 00 af 37 00 00 af 37 00
20 01 00 00 00 00 00 00 00
20 01 00 00 00 06 00 00 00
20 01 91 28 00 00 91 28 00
20 01 9b 2d 00 00 9b 2d 00
20 01 af 39 00 00 af 39 00
20 04 00 00 00 00 00 00 00 00 00
20 04 01 01 01 01 01 01 01 01 01
20 04 01 01 01 01 01 01 01 01 02
20 04 01 01 01 01 01 01 01 01 03
20 04 01 01 01 01 01 01 01 01 04
20 04 01 01 01 01 01 01 01 01 05
20 04 01 01 01 01 01 01 01 01 06
20 04 01 01 01 01 01 01 01 01 07
20 04 01 01 01 01 01 01 01 01 08
20 04 01 01 01 01 01 01 01 02 04
20 04 01 01 01 01 01 01 01 02 08
20 04 01 01 01 01 01 01 01 03 04
20 04 01 01 01 01 01 01 01 03 08
20 04 01 01 01 01 01 01 01 04 08
20 04 01 01 01 01 01 01 01 05 08
20 04 01 01 01 01 01 01 01 06 08
20 04 01 01 01 01 01 01 01 07 08
20 04 01 01 01 01 01 01 01 08 08
20 04 01 01 01 01 01 01 02 03 04
20 04 01 01 01 01 01 01 02 03 05
20 04 01 01 01 01 01 01 02 03 06
20 04 01 01 01 01 01 01 02 03 07
20 04 01 01 01 01 01 01 02 03 08
20 04 01 01 01 01 01 01 02 04 08
20 04 01 01 01 01 01 01 02 05 08
20 04 01 01 01 01 01 01 02 06 08
20 04 01 01 01 01 01 01 02 07 08
20 04 01 01 01 01 01 01 02 08 08
20 04 01 01 01 01 01 01 03 07 08
20 04 01 01 01 01 01 01 03 08 08
20 04 01 01 01 01 01 01 04 07 08
20 04 01 01 01 01 01 01 04 08 08
20 04 01 01 01 01 01 01 05 07 08
20 04 01 01 01 01 01 01 05 08 08
20 04 01 01 01 01 01 01 06 07 08
20 04 01 01 01 01 01 01 06 08 08
20 04 01 01 01 01 01 01 07 08 08
20 04 01 01 01 01 01 01 08 08 08
20 04 01 01 01 01 01 02 06 07 08
20 04 01 01 01 01 01 02 08 08 08
20 04 01 01 01 01 01 03 06 07 08
20 04 01 01 01 01 01 03 08 08 08
20 04 01 01 01 01 01 04 06 07 08
20 04 01 01 01 01 01 04 08 08 08
20 04 01 01 01 01 01 05 08 08 08
20 04 01 01 01 01 01 06 07 08 08
20 04 01 01 01 01 01 06 08 08 08
20 04 01 01 01 01 01 07 08 08 08
20 04 01 01 01 01 01 08 08 08 08
20 04 01 01 01 01 02 04 06 07 08
20 04 01 01 01 01 02 06 07 08 08
20 04 01 01 01 01 02 08 08 08 08
20 04 01 01 01 01 03 04 06 07 08
20 04 01 01 01 01 03 06 07 08 08
20 04 01 01 01 01 03 08 08 08 08
20 04 01 01 01 01 04 06 07 08 08
20 04 01 01 01 01 04 08 08 08 08
20 04 01 01 01 01 05 06 07 08 08
20 04 01 01 01 01 05 08 08 08 08
20 04 01 01 01 01 06 08 08 08 08
20 04 01 01 01 01 07 08 08 08 08
20 04 01 01 01 01 08 08 08 08 08
20 04 01 01 01 02 03 04 06 07 08
20 04 01 01 01 02 05 06 07 08 08
20 04 01 01 01 02 07 08 08 08 08
20 04 01 01 01 03 05 06 07 08 08
20 04 01 01 01 03 07 08 08 08 08
20 04 01 01 01 04 05 06 07 08 08
20 04 01 01 01 04 07 08 08 08 08
20 04 01 01 01 05 07 08 08 08 08
20 04 01 01 01 06 07 08 08 08 08
20 04 01 01 01 07 07 08 08 08 08
20 04 01 01 01 08 08 08 08 08 08
20 04 01 01 02 02 03 04 06 07 08
20 04 01 01 02 04 05 06 07 08 08
20 04 01 01 02 06 07 08 08 08 08
20 04 01 01 03 04 05 06 07 08 08
20 04 01 01 03 06 07 08 08 08 08
20 04 01 01 04 06 07 08 08 08 08
20 04 01 01 05 06 07 08 08 08 08
20 04 01 01 08 08 08 08 08 08 08
20 04 01 02 02 02 03 04 06 07 08
20 04 01 02 02 03 03 04 06 07 08
20 04 01 02 03 03 03 04 06 07 08
20 04 01 02 03 03 04 04 06 07 08
20 04 01 02 03 04 04 04 06 07 08
20 04 01 02 03 04 04 05 06 07 08
20 04 01 02 03 04 04 06 06 07 08
20 04 01 02 03 04 05 06 06 07 08
20 04 01 02 03 04 05 06 07 07 08
20 04 01 02 03 04 05 06 07 08 08
20 04 01 02 05 06 07 08 08 08 08
20 04 01 03 05 06 07 08 08 08 08
20 04 01 04 05 06 07 08 08 08 08
20 04 01 05 05 06 07 08 08 08 08
20 04 01 08 08 08 08 08 08 08 08
20 04 02 02 02 02 03 04 06 07 08
20 04 02 05 05 06 07 08 08 08 08
20 04 03 05 05 06 07 08 08 08 08
20 04 04 05 05 06 07 08 08 08 08
20 04 05 05 05 06 07 08 08 08 08
20 04 08 08 08 08 08 08 08 08 08
21 00
21 01
21 04
22 04
24 00
26 00 00 00
26 01 00 00
50 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 53
50 03 02 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 3d 00 93
50 03 02 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 64 00 ba
50 03 04 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 17 00 6f
50 03 04 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 3d 00 95
50 03 04 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 3e 00 96
50 03 04 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 40 00 98
50 03 04 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 64 00 bc
50 03 06 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 3d 00 97
50 03 06 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 64 00 be
50 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 54
50 04 03 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 29
50 04 03 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 da
50 04 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2b
50 04 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 dc
50 04 08 02 01 ff 00 00 ff 00 80 00 00 00 00 00 00 64 00 41
50 04 08 03 01 ff 00 00 9a 00 ff ff 00 80 00 00 00 64 00 db
50 04 08 04 01 ff 00 00 00 00 ff 9a 00 ff ff 00 80 64 00 db
50 04 08 05 01 ff 00 00 00 ff ff 00 00 ff 9a 00 ff 64 00 5b
50 04 08 05 02 ff 00 80 00 00 00 00 00 00 00 00 00 64 00 46
50 04 08 06 01 ff 00 00 00 ff 00 00 ff ff 00 00 ff 64 00 c2
50 04 08 06 02 9a 00 ff ff 00 80 00 00 00 00 00 00 64 00 e0
50 04 08 07 01 ff 00 00 ff ff 00 00 ff 00 00 ff ff 64 00 c2
50 04 08 07 02 00 00 ff 9a 00 ff ff 00 80 00 00 00 64 00 e0
50 04 08 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2e
50 04 08 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 df
50 05 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 55
50 05 03 02 01 ff 00 00 ff 6a 00 00 00 00 00 00 00 64 00 27
50 05 03 03 01 ff 00 00 ff 6a 00 0b 11 ff 00 00 00 64 00 43
50 05 03 03 01 ff 00 00 ff 6a 00 ff ff 00 00 00 00 64 00 26
50 05 03 03 01 ff 00 00 ff 6a 00 ff ff ff 00 00 00 64 00 25
50 05 03 04 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 26
50 05 03 05 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 27
50 05 03 05 02 00 ff ff 00 00 00 00 00 00 00 00 00 64 00 c1
50 05 03 06 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 28
50 05 03 06 02 00 ff ff 00 00 ff 00 00 00 00 00 00 64 00 c1
50 05 03 07 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 29
50 05 03 07 02 00 ff ff 00 00 ff 9a 00 ff 00 00 00 64 00 5b
50 05 03 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2a
50 05 03 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 db
50 05 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2c
50 05 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 dd
50 05 08 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2f
50 05 08 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 e0
50 06 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 56
50 06 06 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 2e
50 06 06 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 08 64 00 67
50 06 08 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 30
50 06 08 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 08 64 00 69
50 06 0a 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 32
50 06 0a 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 08 64 00 6b
50 0b 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 5b
50 0b 00 01 01 02 ff 00 00 00 00 00 00 00 00 00 00 64 00 c2
50 0b 00 01 01 ff 0a 08 00 00 00 00 00 00 00 00 00 64 00 d2
50 0b 00 01 01 ff e0 9d 00 00 00 00 00 00 00 00 00 14 00 ed
50 0b 00 01 01 ff e0 9d 00 00 00 00 00 00 00 00 00 64 00 3d
50 0b 00 01 01 ff ff ff 00 00 00 00 00 00 00 00 00 64 00 be
50 0c 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 5c
50 10 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 60
50 10 03 03 01 ff 00 00 00 ff 00 ff ff ff 00 00 00 64 00 c6
50 10 05 03 01 ff 00 00 00 ff 00 ff ff ff 00 00 00 64 00 c8
50 10 08 02 01 ff 00 00 ff ff ff 00 00 00 00 00 00 64 00 cb
50 10 08 03 01 ff 00 00 00 ff 00 ff ff ff 00 00 00 64 00 cb
50 10 08 03 01 ff 00 00 ff 6a 00 ff ff 00 00 00 00 64 00 36
50 11 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 61
50 13 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 63
50 13 00 01 01 00 ff ff 00 00 00 00 00 00 00 00 00 64 01 c8
50 13 00 01 01 00 ff ff 00 00 00 00 00 00 00 00 00 64 02 c9
50 14 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 64
50 15 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 65
50 15 03 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 3a
50 15 03 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 eb
50 15 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 3c
50 15 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 ed
50 15 08 02 01 ff 00 00 ff 00 80 00 00 00 00 00 00 64 00 52
50 15 08 03 01 ff 00 00 9a 00 ff ff 00 80 00 00 00 64 00 ec
50 15 08 04 01 ff 00 00 00 00 ff 9a 00 ff ff 00 80 64 00 ec
50 15 08 05 01 ff 00 00 00 ff ff 00 00 ff 9a 00 ff 64 00 6c
50 15 08 05 02 ff 00 80 00 00 00 00 00 00 00 00 00 64 00 57
50 15 08 06 01 ff 00 00 00 ff 00 00 ff ff 00 00 ff 64 00 d3
50 15 08 06 02 9a 00 ff ff 00 80 00 00 00 00 00 00 64 00 f1
50 15 08 07 01 ff 00 00 ff ff 00 00 ff 00 00 ff ff 64 00 d3
50 15 08 07 02 00 00 ff 9a 00 ff ff 00 80 00 00 00 64 00 f1
50 15 08 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 3f
50 15 08 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 f0
51 00
51 03
51 04
51 05
51 06
51 0b
51 0c
51 10
51 11
51 13
51 14
51 15
52 00
60 00 01 00 3e
60 01 00 00 3e
60 01 01 00 3e
61
a5 a0 00 00
'''.strip().splitlines()
# Sent by the app but its purpose is unknown, so the CLI must not send it.
UNKNOWN = {'52 00'}


def h(text):
    return bytes.fromhex(text.replace(' ', ''))


class Whitelist(unittest.TestCase):
    def test_app_writes_pass(self):
        for line in APP_WRITES:
            f = h(line)
            if f[0] == 0x50 and f[1] in (0x0c, 0x11, 0x14) and any(f[2:19]):
                continue
            with self.subTest(line):
                if line in UNKNOWN:
                    self.assertRaises(p.UnsafeFrame, p.check_frame, f)
                else:
                    p.check_frame(f)

    def test_rejects(self):
        bad = [
            '52 00', 'df', '06', 'ff 0a', '13', '20 02 00 00 00 00 00 00 00',
            '20 00 af 37 01 00 af 37 01',            # preset not seen in the app
            '20 04 00 05 05 06 07 08 08 08 08',      # level 0
            '20 04 09 05 05 06 07 08 08 08 08',      # level 9
            '21 02', '26 02 00 00', '10 01', '51 01',
            '60 00 01 01 00',                        # minutes > 255
            '60 00 01 00 00',                        # enabled with 0 minutes
        ]
        for line in bad:
            with self.subTest(line):
                self.assertRaises(p.UnsafeFrame, p.check_frame, h(line))

    def test_led_rejects(self):
        def led(body):
            f = h(body)
            return f + bytes([p.checksum(f)])
        ok = '50 05 03 02 01 ff 00 00 ff 6a 00 00 00 00 00 00 00 64 00'
        p.check_frame(led(ok))
        bad = [
            ok.replace('50 05 03', '50 05 04'),                     # speed
            '50 05 03 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 64 00',  # 1 colour
            ok.replace('64 00', '13 00'),                           # brightness 19
            ok.replace('64 00', '65 00'),                           # brightness 101
            ok.replace('64 00', '64 01'),                           # colour mode
            ok.replace('ff 6a 00 00 00 00', 'ff 6a 00 01 00 00'),   # junk slot
            '50 11 00 02 01 ff 00 00 00 00 ff 00 00 00 00 00 00 64 00',  # pulsating
            '50 12 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00',  # no effect 12
        ]
        for body in bad:
            with self.subTest(body):
                self.assertRaises(p.UnsafeFrame, p.check_frame, led(body))
        f = led(ok)
        self.assertRaises(p.UnsafeFrame, p.check_frame, f[:-1] + bytes([f[-1] ^ 1]))


class Builders(unittest.TestCase):
    def test_presets(self):
        self.assertEqual(p.mode_preset('smart', 'silent'), h('20 00 91 28 00 06 91 28 00'))
        self.assertEqual(p.mode_preset('fixed', 'max'), h('20 01 af 39 00 00 af 39 00'))
        self.assertEqual(p.mode_tab('custom'), h('20 04') + bytes(9))

    def test_curve(self):
        self.assertEqual(p.custom_curve([4, 5, 5, 6, 7, 8, 8, 8, 8]), h('20 04 04 05 05 06 07 08 08 08 08'))
        self.assertRaises(ValueError, p.custom_curve, [5] * 8)

    def test_shutdown(self):
        self.assertEqual(p.shutdown(False, True, 62), h('60 00 01 00 3e'))
        self.assertEqual(p.shutdown(True, False, 62), h('60 01 00 00 3e'))

    def test_led_select(self):
        self.assertEqual(p.led_select(0x03), h('50 03' + ' 00' * 17 + ' 53'))

    def test_led_eight_colours(self):
        colors = [tuple(h(c)) for c in
                  ('ff0000', 'ff6a00', 'ffff00', '00ff00', '00ffff', '0000ff', '9a00ff', 'ff0080')]
        s = p.LedSettings(effect=0x15, speed=5, colors=colors, brightness=100)
        self.assertEqual(p.led_frames(s), [
            h('50 15 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 3c'),
            h('50 15 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 ed'),
        ])

    def test_led_rhythm(self):
        s = p.LedSettings(effect=0x13, colors=[(0, 0xff, 0xff)], brightness=100, color_mode=2)
        self.assertEqual(p.led_frames(s), [h('50 13 00 01 01 00 ff ff 00 00 00 00 00 00 00 00 00 64 02 c9')])

    def test_led_reply(self):
        s = p.parse_led_reply([
            h('51 15 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 01 00'),
            h('51 15 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 01 00'),
        ])
        self.assertEqual((s.effect, s.speed, s.brightness, len(s.colors)), (0x15, 5, 100, 8))
        self.assertEqual(s.colors[-1], (0xff, 0x00, 0x80))
        self.assertEqual(s.speed_name(), 'medium')
        one = p.parse_led_reply([h('51 0c 00 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 00 01 00')])
        self.assertEqual(one.colors, [(0xff, 0, 0)])


class Status(unittest.TestCase):
    def test_example(self):
        s = p.parse_status(h('13 00 02 91 28 00 06 08 04 2a 2d c9 25 32 ff 0c 01 03 00 00'))
        self.assertEqual(s.mode, p.SMART)
        self.assertEqual(s.preset_name(), 'silent')
        self.assertEqual((s.cold_setpoint, s.fan_setpoint, s.fan_rpm), (145, 4000, 3750))
        self.assertEqual(s.cold_temp, 8.4)
        self.assertTrue(s.light)
        self.assertEqual(s.quick_switch, p.FIXED)
        self.assertFalse(s.overclock)

    def test_negative_temps(self):
        # app screen value, [7], [8]
        for want, hi, lo in ((-5.1, 0xfb, 1), (-8.4, 0xf8, 4), (-9.1, 0xf7, 1), (6.1, 6, 1)):
            self.assertEqual(round(p.cold_temp(hi, lo), 1), want)

    def test_fixed_preset(self):
        s = p.parse_status(h('13 00 12 9b 2d 00 00 08 04 2a 2d c9 25 32 ff 04 04 83 00 00'))
        self.assertEqual((s.mode, s.preset_name(), s.overclock), (p.FIXED, 'strong', True))


if __name__ == '__main__':
    unittest.main()
