//! Control flows against the simulated device.

use std::time::Duration;

use b9x_ble::control::{self, LedChange};
use b9x_ble::{Device, Error, fake};
use b9x_proto::{self as p, Mode, Preset};

async fn open() -> Device {
    Device::open(fake::connect(Duration::from_millis(20)), false).await.unwrap()
}

#[tokio::test]
async fn handshake_and_queries() {
    let dev = open().await;
    assert_eq!(dev.version().as_deref(), Some("0.3.0"));
    assert_eq!(dev.mac().await.unwrap(), "00:00:00:00:B9:0F");
    assert_eq!(dev.curve().await.unwrap(), vec![5, 5, 5, 6, 7, 8, 8, 8, 8]);
    let s = dev.status().await.unwrap();
    assert_eq!((s.mode, s.preset()), (Mode::Smart.code(), Some(Preset::Silent)));
}

#[tokio::test]
async fn mode_change_sets_quick_switch_to_previous() {
    let dev = open().await;
    control::set_mode(&dev, Mode::Fixed, Some(Preset::Strong), None).await.unwrap();
    dev.status().await.unwrap();
    let s = dev.status().await.unwrap();
    assert_eq!((s.mode, s.preset(), s.quick_switch), (Mode::Fixed.code(), Some(Preset::Strong), Mode::Smart.code()));

    control::set_mode(&dev, Mode::Custom, None, Some(&[1, 2, 3, 4, 5, 6, 7, 8, 8])).await.unwrap();
    assert_eq!(dev.curve().await.unwrap(), vec![1, 2, 3, 4, 5, 6, 7, 8, 8]);
    assert_eq!(dev.status().await.unwrap().quick_switch, Mode::Fixed.code());

    let e = control::set_quick_switch(&dev, Mode::Custom).await.unwrap_err();
    assert!(matches!(e, Error::Invalid(_)));
    assert!(control::set_mode(&dev, Mode::Smart, None, None).await.is_err());
    assert!(control::set_mode(&dev, Mode::Smart, Some(Preset::Strong), None).await.is_err());
}

#[tokio::test]
async fn led_settings_roundtrip() {
    let dev = open().await;
    let e = p::effect_by_name("breathing").unwrap();
    let colors = vec![[0xff, 0, 0], [0, 0, 0xff]];
    let change = LedChange { speed: Some(2), colors: Some(colors.clone()), brightness: Some(60), ..Default::default() };
    let s = control::set_led(&dev, e, &change).await.unwrap();
    assert_eq!((s.effect, s.speed_name(), s.brightness), (e.code, Some("fast"), 60));
    assert_eq!(s.colors, colors);
    assert_eq!(dev.led(0).await.unwrap().effect, e.code);

    let rhythm = p::effect_by_name("rhythm").unwrap();
    let change = LedChange { rhythm_colorful: Some(true), ..Default::default() };
    assert_eq!(control::set_led(&dev, rhythm, &change).await.unwrap().color_mode, p::RHYTHM_COLORFUL);

    let smart = p::effect_by_name("smart").unwrap();
    assert!(control::set_led(&dev, smart, &LedChange { brightness: Some(50), ..Default::default() }).await.is_err());
    let too_many = LedChange { colors: Some(vec![[1, 2, 3]; 2]), ..Default::default() };
    assert!(control::set_led(&dev, p::effect_by_name("always-on").unwrap(), &too_many).await.is_err());
}

#[tokio::test]
async fn shutdown_and_oc() {
    let dev = open().await;
    let sd = control::set_shutdown(&dev, Some((90, false))).await.unwrap();
    assert_eq!((sd.enable, sd.repeat, sd.minutes), (true, false, 90));
    assert_eq!(dev.status().await.unwrap().shutdown_left, 90);
    let sd = control::set_shutdown(&dev, None).await.unwrap();
    assert_eq!((sd.enable, sd.minutes), (false, 90));

    assert!(control::set_overclock(&dev, true, false).await.is_err());
    control::set_overclock(&dev, true, true).await.unwrap();
    dev.status().await.unwrap();
    assert!(dev.status().await.unwrap().overclock);
}

#[tokio::test]
async fn disconnect_fails_pending_work() {
    let dev = open().await;
    dev.disconnect().await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(matches!(dev.mac().await, Err(Error::Disconnected(_))));
}

async fn open_with(opts: fake::Options) -> Device {
    Device::open(fake::connect_with(opts), false).await.unwrap()
}

/// Status pushes three behind, like the real device right after a write.
fn lagging() -> fake::Options {
    fake::Options { period: Duration::from_millis(20), status_lag: 3, ..fake::Options::default() }
}

async fn settled(dev: &Device) -> b9x_proto::Status {
    for _ in 0..5 {
        dev.status().await.unwrap();
    }
    dev.status().await.unwrap()
}

#[tokio::test]
async fn quick_switch_follows_writes_not_stale_status() {
    let dev = open_with(lagging()).await;
    control::set_mode(&dev, Mode::Fixed, Some(Preset::Strong), None).await.unwrap();
    // Back at once, before any push shows Fixed.
    control::set_mode(&dev, Mode::Smart, Some(Preset::Silent), None).await.unwrap();
    let s = settled(&dev).await;
    assert_eq!((s.mode, s.quick_switch), (Mode::Smart.code(), Mode::Fixed.code()));

    // The quick switch check also goes by what was written.
    control::set_mode(&dev, Mode::Custom, None, None).await.unwrap();
    let e = control::set_quick_switch(&dev, Mode::Custom).await.unwrap_err();
    assert!(matches!(e, Error::Invalid(_)), "{e}");
}

#[tokio::test]
async fn curve_only_in_custom_mode() {
    let dev = open_with(lagging()).await;
    let levels = [1, 2, 3, 4, 5, 6, 7, 8, 8];
    // A curve frame switches to Custom, so it must not go out in another mode.
    let e = control::set_curve(&dev, &levels).await.unwrap_err();
    assert!(matches!(e, Error::Invalid(_)), "{e}");
    assert_eq!(settled(&dev).await.mode, Mode::Smart.code());

    control::set_mode(&dev, Mode::Custom, None, None).await.unwrap();
    control::set_curve(&dev, &levels).await.unwrap();
    assert_eq!(dev.curve().await.unwrap(), levels);
    control::set_mode(&dev, Mode::Fixed, None, None).await.unwrap();
    assert!(control::set_curve(&dev, &levels).await.is_err());
}

#[tokio::test]
async fn shutdown_off_with_long_stored_duration() {
    let opts = fake::Options { period: Duration::from_millis(20), shutdown: (false, true, 300), ..Default::default() };
    let dev = open_with(opts).await;
    assert_eq!(dev.shutdown().await.unwrap().minutes, 300);
    let sd = control::set_shutdown(&dev, None).await.unwrap();
    assert_eq!((sd.enable, sd.minutes), (false, p::SHUTDOWN_MAX_MIN));
}

#[tokio::test]
async fn led_speed_out_of_range_is_an_error() {
    let dev = open().await;
    let change = LedChange { speed: Some(3), ..Default::default() };
    let e = control::set_led(&dev, p::effect_by_name("breathing").unwrap(), &change).await.unwrap_err();
    assert!(matches!(e, Error::Invalid(_)), "{e}");
}

#[tokio::test]
async fn wrong_mac_in_handshake_is_refused() {
    // Answers as another device: nothing after the handshake may be written.
    let opts = fake::Options { reported_mac: [1, 2, 3, 4, 5, 6], ..fake::Options::default() };
    let e = Device::open(fake::connect_with(opts), false).await.err().expect("must refuse");
    assert!(matches!(e, Error::NotB9x(_)), "{e}");
}

#[tokio::test]
async fn unknown_product_id_is_a_warning() {
    let dev = open().await;
    assert_eq!(dev.identity_warning(), None);
    let opts = fake::Options { product_id: [0x19, 0x31, 0xf0, 0xf1, 0xf2, 0xf3, 0xf4], ..fake::Options::default() };
    let dev = Device::open(fake::connect_with(opts), false).await.unwrap();
    assert!(dev.identity_warning().is_some_and(|w| w.contains("19 31")));
}
