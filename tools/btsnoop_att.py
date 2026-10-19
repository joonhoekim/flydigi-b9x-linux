#!/usr/bin/env python3
"""Print the ATT traffic in an Android btsnoop_hci.log (H4 datalink).

Shows LE connections, discovered characteristics (handle -> UUID) and
writes / notifications with their payload in hex. Stdlib only.

usage: btsnoop_att.py LOG [--addr AA:BB:..] [--all]
  --all   also print reads, MTU exchange and other ATT opcodes
"""
import argparse
import datetime
import struct
import sys

ATT_CID = 0x0004
OPCODES = {
    0x01: 'ERR', 0x02: 'MTU_REQ', 0x03: 'MTU_RSP', 0x04: 'FIND_INFO_REQ', 0x05: 'FIND_INFO_RSP',
    0x08: 'READ_TYPE_REQ', 0x09: 'READ_TYPE_RSP', 0x0a: 'READ_REQ', 0x0b: 'READ_RSP',
    0x10: 'READ_GROUP_REQ', 0x11: 'READ_GROUP_RSP', 0x12: 'WRITE_REQ', 0x13: 'WRITE_RSP',
    0x1b: 'NOTIFY', 0x1d: 'INDICATE', 0x1e: 'CONFIRM', 0x52: 'WRITE_CMD',
}
MAIN = {0x12, 0x52, 0x1b, 0x1d}
# btsnoop timestamps are microseconds since 0000-01-01
EPOCH_DELTA_US = 0x00dcddb30f2f8000


def uuid_str(raw):
    if len(raw) == 2:
        return '%04x' % struct.unpack('<H', raw)
    return raw[::-1].hex()


def fmt_addr(raw):
    return ':'.join('%02X' % b for b in raw[::-1])


def records(path):
    with open(path, 'rb') as f:
        hdr = f.read(16)
        if hdr[:8] != b'btsnoop\0':
            sys.exit('not a btsnoop file (btsnooz needs decoding first)')
        while True:
            rec = f.read(24)
            if len(rec) < 24:
                return
            _, incl, flags, _, ts = struct.unpack('>IIIIq', rec)
            yield flags, ts, f.read(incl)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('log')
    ap.add_argument('--addr', help='only show this device')
    ap.add_argument('--all', action='store_true')
    args = ap.parse_args()

    conns = {}      # conn handle -> address
    chars = {}      # (address, value handle) -> uuid
    partial = {}    # conn handle -> [expected l2cap len, buffer, direction, ts]

    def handle_att(ch, rx, ts, pdu):
        addr = conns.get(ch, 'conn%d' % ch)
        if args.addr and addr.upper() != args.addr.upper():
            return
        op = pdu[0]
        name = OPCODES.get(op, '0x%02x' % op)
        if op == 0x09 and len(pdu) > 2:
            n = pdu[1]
            for i in range(2, len(pdu) - n + 1, n):
                e = pdu[i:i + n]
                if n in (7, 21):  # characteristic declaration
                    props, vh = e[2], struct.unpack('<H', e[3:5])[0]
                    chars[(addr, vh)] = uuid_str(e[5:])
                    print('    char 0x%04x %s props=0x%02x' % (vh, uuid_str(e[5:]), props))
        if op == 0x05 and len(pdu) > 2:
            n = 4 if pdu[1] == 1 else 18
            for i in range(2, len(pdu) - n + 1, n):
                h = struct.unpack('<H', pdu[i:i + 2])[0]
                chars.setdefault((addr, h), uuid_str(pdu[i + 2:i + n]))
        if op not in MAIN and not args.all:
            return
        # Android stores device local time, so format it without a timezone shift
        t = datetime.datetime.fromtimestamp((ts - EPOCH_DELTA_US) / 1e6, datetime.timezone.utc)
        t = t.strftime('%H:%M:%S.%f')[:-3]
        arrow = '<-' if rx else '->'
        body = pdu[1:]
        if op in MAIN or op in (0x0a, 0x0b):
            h = struct.unpack('<H', body[:2])[0] if op != 0x0b else None
            if h is not None:
                body = body[2:]
                tag = '0x%04x(%s)' % (h, chars.get((addr, h), '?')[-12:] if (addr, h) in chars else '?')
            else:
                tag = ''
        else:
            tag = ''
        print('%s %s %s %-10s %s %s' % (t, addr, arrow, name, tag, body.hex(' ')))

    for flags, ts, data in records(args.log):
        if not data:
            continue
        rx = bool(flags & 1)
        kind, data = data[0], data[1:]
        if kind == 0x04 and len(data) > 2 and data[0] == 0x3e:  # LE meta event
            sub = data[2]
            if sub in (0x01, 0x0a) and data[3] == 0:
                ch = struct.unpack('<H', data[4:6])[0]
                conns[ch] = fmt_addr(data[8:14])
                print('== connected conn=%d addr=%s' % (ch, conns[ch]))
        elif kind == 0x04 and len(data) > 5 and data[0] == 0x05:  # disconnection complete
            ch = struct.unpack('<H', data[3:5])[0]
            print('== disconnected conn=%d addr=%s reason=0x%02x' % (ch, conns.get(ch, '?'), data[5]))
        elif kind == 0x02 and len(data) >= 4:  # ACL
            hf, ln = struct.unpack('<HH', data[:4])
            ch, pb = hf & 0x0fff, (hf >> 12) & 0x3
            payload = data[4:4 + ln]
            if pb == 0x1:  # continuation fragment
                p = partial.get(ch)
                if not p:
                    continue
                p[1] += payload
            else:
                if len(payload) < 4:
                    continue
                l2len, cid = struct.unpack('<HH', payload[:4])
                if cid != ATT_CID:
                    partial.pop(ch, None)
                    continue
                p = partial[ch] = [l2len, payload[4:], rx, ts]
            if len(p[1]) >= p[0]:
                del partial[ch]
                handle_att(ch, p[2], p[3], p[1][:p[0]])


if __name__ == '__main__':
    main()
