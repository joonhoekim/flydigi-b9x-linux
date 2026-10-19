//! Codec and whitelist checks against frames captured from the official app.

use b9x_proto::*;

/// Every distinct write the app made in captures 1-4, shared with the Python tests.
const APP_WRITES: &str = include_str!("../../../testdata/app_writes.txt");
/// Sent by the app but its purpose is unknown, so we must not send it.
const UNKNOWN: &[&str] = &["52 00"];

fn h(text: &str) -> Vec<u8> {
    text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
}

fn app_writes() -> impl Iterator<Item = &'static str> {
    APP_WRITES.lines().filter(|l| !l.is_empty() && !l.starts_with('#'))
}

#[test]
fn app_writes_pass() {
    let mut n = 0;
    for line in app_writes() {
        let f = h(line);
        // Settings the app wrote for select-only effects; we only ever select them.
        if f[0] == 0x50 && [0x0c, 0x11, 0x14].contains(&f[1]) && f[2..19].iter().any(|&b| b != 0) {
            continue;
        }
        let r = check_frame(&f);
        if UNKNOWN.contains(&line) {
            assert!(r.is_err(), "{line} must be rejected");
        } else {
            assert!(r.is_ok(), "{line}: {r:?}");
        }
        n += 1;
    }
    assert!(n > 200);
}

#[test]
fn rejects() {
    let bad = [
        "52 00",
        "df",
        "06",
        "ff 0a",
        "13",
        "20 02 00 00 00 00 00 00 00",
        "20 00 af 37 01 00 af 37 01",       // preset not seen in the app
        "20 04 00 05 05 06 07 08 08 08 08", // level 0
        "20 04 09 05 05 06 07 08 08 08 08", // level 9
        "21 02",
        "26 02 00 00",
        "10 01",
        "51 01",
        "60 00 01 01 00", // minutes > 255
        "60 00 01 00 00", // enabled with 0 minutes
        "",
    ];
    for line in bad {
        assert!(check_frame(&h(line)).is_err(), "{line} must be rejected");
        assert!(Frame::check(h(line)).is_err());
    }
}

#[test]
fn led_rejects() {
    let led = |body: &str| {
        let mut f = h(body);
        f.push(checksum(&f));
        f
    };
    let ok = "50 05 03 02 01 ff 00 00 ff 6a 00 00 00 00 00 00 00 64 00";
    check_frame(&led(ok)).unwrap();
    let bad = [
        ok.replace("50 05 03", "50 05 04"),                                // speed
        "50 05 03 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 64 00".into(), // 1 colour
        ok.replace("64 00", "13 00"),                                      // brightness 19
        ok.replace("64 00", "65 00"),                                      // brightness 101
        ok.replace("64 00", "64 01"),                                      // colour mode
        ok.replace("ff 6a 00 00 00 00", "ff 6a 00 01 00 00"),              // junk slot
        "50 11 00 02 01 ff 00 00 00 00 ff 00 00 00 00 00 00 64 00".into(), // pulsating
        "50 12 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00".into(), // no effect 12
    ];
    for body in &bad {
        assert!(check_frame(&led(body)).is_err(), "{body} must be rejected");
    }
    let mut f = led(ok);
    *f.last_mut().unwrap() ^= 1;
    assert!(check_frame(&f).is_err());
}

#[test]
fn presets() {
    assert_eq!(mode_preset(Mode::Smart, Preset::Silent).unwrap(), h("20 00 91 28 00 06 91 28 00"));
    assert_eq!(mode_preset(Mode::Fixed, Preset::Max).unwrap(), h("20 01 af 39 00 00 af 39 00"));
    assert_eq!(mode_tab(Mode::Custom).unwrap(), h("20 04 00 00 00 00 00 00 00 00 00"));
    assert!(mode_preset(Mode::Smart, Preset::Strong).is_err());
    assert!(mode_tab(Mode::Smart).is_err());
}

#[test]
fn curve() {
    assert_eq!(custom_curve(&[4, 5, 5, 6, 7, 8, 8, 8, 8]).unwrap(), h("20 04 04 05 05 06 07 08 08 08 08"));
    assert!(custom_curve(&[5; 8]).is_err());
    assert!(custom_curve(&[0, 5, 5, 6, 7, 8, 8, 8, 8]).is_err());
}

#[test]
fn shutdown_frames() {
    assert_eq!(shutdown(false, true, 62).unwrap(), h("60 00 01 00 3e"));
    assert_eq!(shutdown(true, false, 62).unwrap(), h("60 01 00 00 3e"));
    assert!(shutdown(false, true, 0).is_err());
    assert!(shutdown(false, true, 256).is_err());
}

#[test]
fn led_select_frame() {
    assert_eq!(led_select(0x03), h(&format!("50 03{} 53", " 00".repeat(17))));
}

#[test]
fn led_eight_colours() {
    let colors: Vec<Rgb> = ["ff0000", "ff6a00", "ffff00", "00ff00", "00ffff", "0000ff", "9a00ff", "ff0080"]
        .iter()
        .map(|c| {
            let b = (0..3).map(|i| u8::from_str_radix(&c[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<_>>();
            [b[0], b[1], b[2]]
        })
        .collect();
    let s = LedSettings { effect: 0x15, speed: 5, colors, brightness: 100, color_mode: 0 };
    assert_eq!(
        led_frames(&s),
        vec![
            h("50 15 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 00 3c"),
            h("50 15 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 00 ed"),
        ]
    );
}

#[test]
fn led_rhythm() {
    let s = LedSettings { effect: 0x13, speed: 0, colors: vec![[0, 0xff, 0xff]], brightness: 100, color_mode: 2 };
    assert_eq!(led_frames(&s), vec![h("50 13 00 01 01 00 ff ff 00 00 00 00 00 00 00 00 00 64 02 c9")]);
}

#[test]
fn led_reply() {
    let s = parse_led_reply(&[
        h("51 15 05 08 01 ff 00 00 ff 6a 00 ff ff 00 00 ff 00 64 01 00"),
        h("51 15 05 08 02 00 ff ff 00 00 ff 9a 00 ff ff 00 80 64 01 00"),
    ]);
    assert_eq!((s.effect, s.speed, s.brightness, s.colors.len()), (0x15, 5, 100, 8));
    assert_eq!(s.colors[7], [0xff, 0x00, 0x80]);
    assert_eq!(s.speed_name(), Some("medium"));
    let one = parse_led_reply(&[h("51 0c 00 01 01 ff 00 00 00 00 00 00 00 00 00 00 00 00 01 00")]);
    assert_eq!(one.colors, vec![[0xff, 0, 0]]);
}

#[test]
fn status_example() {
    let s = parse_status(&h("13 00 02 91 28 00 06 08 04 2a 2d c9 25 32 ff 0c 01 03 00 00")).unwrap();
    assert_eq!(s.mode, Mode::Smart.code());
    assert_eq!(s.preset(), Some(Preset::Silent));
    assert_eq!((s.cold_setpoint, s.fan_setpoint, s.fan_rpm), (145, 4000, 3750));
    assert_eq!(s.cold_temp, 8.4);
    assert!(s.light);
    assert_eq!(s.quick_switch, Mode::Fixed.code());
    assert!(!s.overclock);
    assert_eq!(s.shutdown_left, 0);
    let s = parse_status(&h("13 00 02 91 28 00 06 06 00 21 37 82 27 3c ff 0c 04 03 00 77")).unwrap();
    assert_eq!(s.shutdown_left, 119);
}

#[test]
fn negative_temps() {
    // app screen value, [7], [8]
    for (want, hi, lo) in [(-5.1, 0xfb, 1), (-8.4, 0xf8, 4), (-9.1, 0xf7, 1), (6.1, 6, 1)] {
        assert_eq!((cold_temp(hi, lo) * 10.0).round() / 10.0, want);
    }
}

#[test]
fn fixed_preset() {
    let s = parse_status(&h("13 00 12 9b 2d 00 00 08 04 2a 2d c9 25 32 ff 04 04 83 00 00")).unwrap();
    assert_eq!((s.mode, s.preset(), s.overclock), (Mode::Fixed.code(), Some(Preset::Strong), true));
}

#[test]
fn replies() {
    assert_eq!(parse_mac(&h("24 83 f4 c5 64 7f 9c")).unwrap(), "9C:7F:64:C5:F4:83");
    assert_eq!(parse_curve(&h("22 04 05 05 05 06 07 08 08 08 08")).unwrap(), vec![5, 5, 5, 6, 7, 8, 8, 8, 8]);
    let sd = parse_shutdown(&h("61 01 01 00 3e")).unwrap();
    assert_eq!((sd.repeat, sd.enable, sd.minutes), (true, true, 62));
}
