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
