//! Changes that take more than one frame, done the way the official app does them.

use b9x_proto::{self as p, LedSettings, Mode, Preset, Rgb, ShutdownInfo};

use crate::{Device, Error, Result};

fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(msg.into()))
}

/// Switch cooling mode. With neither preset nor curve the device keeps what it has
/// stored for that mode (Smart always needs a preset).
pub async fn set_mode(dev: &Device, mode: Mode, preset: Option<Preset>, curve: Option<&[u8]>) -> Result<()> {
    if mode == Mode::Smart && preset.is_none() {
        return invalid("smart needs --preset max or silent");
    }
    if let Some(pr) = preset {
        if p::preset_values(mode, pr).is_none() {
            return invalid(format!("{} has no preset {}", mode.name(), pr.name()));
        }
    }
    if curve.is_some() && mode != Mode::Custom {
        return invalid("--curve is for custom mode");
    }
    let before = dev.status().await?;
    let frame = match (curve, preset) {
        (Some(c), _) => p::custom_curve(c)?,
        (None, Some(pr)) => p::mode_preset(mode, pr)?,
        (None, None) => p::mode_tab(mode)?,
    };
    dev.write(&frame).await?;
    // Like the app: the long-press target becomes the mode we just left.
    if let Some(prev) = Mode::from_code(before.mode) {
        if prev != mode {
            dev.write(&p::quick_switch(prev)).await?;
        }
    }
    Ok(())
}

/// Write a custom curve while already in Custom mode, as the app does on every knob move.
pub async fn set_curve(dev: &Device, levels: &[u8]) -> Result<()> {
    dev.write(&p::custom_curve(levels)?).await
}

pub async fn set_quick_switch(dev: &Device, mode: Mode) -> Result<()> {
    let s = dev.status().await?;
    if mode.code() == s.mode {
        return invalid(format!("quick switch cannot be the current mode ({})", mode.name()));
    }
    if mode.code() != s.quick_switch {
        dev.write(&p::quick_switch(mode)).await?;
    }
    Ok(())
}

pub async fn set_light(dev: &Device, on: bool) -> Result<()> {
    dev.write(&p::light(on)).await
}

pub const OC_WARNING: &str = "OC runs at full power with frost protection off; condensation can form.";

/// `confirmed` must come from the user after showing [`OC_WARNING`].
pub async fn set_overclock(dev: &Device, on: bool, confirmed: bool) -> Result<()> {
    if on && !confirmed {
        return invalid(format!("{OC_WARNING} Add --yes to turn it on."));
    }
    dev.write(&p::overclock(on)).await
}

/// Schedule a shutdown, or cancel it with `None`. Returns what the device reports after.
pub async fn set_shutdown(dev: &Device, schedule: Option<(u16, bool)>) -> Result<ShutdownInfo> {
    match schedule {
        Some((minutes, repeat)) => dev.write(&p::shutdown(repeat, true, minutes)?).await?,
        None => {
            // Keep the last duration like the app. The only off frame captured has
            // repeat set, so send that one regardless of the current flag.
            let cur = dev.shutdown().await?;
            if cur.enable {
                dev.write(&p::shutdown(true, false, cur.minutes)?).await?;
            }
        }
    }
    dev.shutdown().await
}

/// Settings to change on top of what the device has stored for the effect.
#[derive(Debug, Clone, Default)]
pub struct LedChange {
    /// Index into [`p::SPEED_NAMES`].
    pub speed: Option<usize>,
    pub colors: Option<Vec<Rgb>>,
    pub brightness: Option<u8>,
    pub rhythm_colorful: Option<bool>,
}

impl LedChange {
    pub fn is_empty(&self) -> bool {
        self.speed.is_none() && self.colors.is_none() && self.brightness.is_none() && self.rhythm_colorful.is_none()
    }
}

/// Select `effect` for the current mode and apply `change`. Returns the stored settings after.
pub async fn set_led(dev: &Device, effect: &p::Effect, change: &LedChange) -> Result<LedSettings> {
    if !change.is_empty() && !effect.writable {
        return invalid(format!("{} has no settings; select it without options", effect.name));
    }
    if change.speed.is_some() && effect.speeds.is_none() {
        return invalid(format!("{} has no speed", effect.name));
    }
    if let Some(colors) = &change.colors {
        let Some((lo, hi)) = effect.colors else {
            return invalid(format!("{} colours cannot be changed", effect.name));
        };
        if !(lo..=hi).contains(&colors.len()) {
            return invalid(format!("{} takes {}-{} colours", effect.name, lo, hi));
        }
    }
    if let Some(b) = change.brightness {
        if !(p::BRIGHTNESS_MIN..=p::BRIGHTNESS_MAX).contains(&b) {
            return invalid(format!("brightness must be {}-{}", p::BRIGHTNESS_MIN, p::BRIGHTNESS_MAX));
        }
    }
    if change.rhythm_colorful.is_some() && effect.name != "rhythm" {
        return invalid("--rhythm is for the rhythm effect");
    }

    dev.write(&p::led_select(effect.code)).await?;
    if !change.is_empty() {
        let mut s = dev.led(effect.code).await?;
        match (change.speed, effect.speeds) {
            (Some(i), Some(speeds)) => s.speed = speeds[i],
            (_, None) => s.speed = 0,
            _ => {}
        }
        if let Some(colors) = &change.colors {
            s.colors = colors.clone();
        }
        if let Some(b) = change.brightness {
            s.brightness = b;
        }
        if effect.name == "rhythm" {
            match change.rhythm_colorful {
                Some(true) => s.color_mode = p::RHYTHM_COLORFUL,
                Some(false) => s.color_mode = p::RHYTHM_SINGLE,
                None if ![p::RHYTHM_SINGLE, p::RHYTHM_COLORFUL].contains(&s.color_mode) => {
                    s.color_mode = p::RHYTHM_SINGLE
                }
                None => {}
            }
        } else {
            s.color_mode = 0;
        }
        // Check every frame before sending any, so a bad second frame cannot leave
        // the device with half a write.
        let frames = p::led_frames(&s);
        for f in &frames {
            p::check_frame(f)?;
        }
        for f in &frames {
            dev.write(f).await?;
        }
    }
    dev.led(effect.code).await
}
