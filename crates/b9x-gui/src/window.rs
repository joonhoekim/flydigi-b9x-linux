//! Main window. Widgets follow the device: every status push and reply updates them,
//! and user changes go to the worker as commands.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use async_channel::Sender;
use b9x_ble::control::{LedChange, OC_WARNING};
use b9x_proto::{self as p, LedSettings, Mode, Preset, ShutdownInfo, Status};
use gtk::{gdk, glib};

use crate::chart::Chart;
use crate::worker::{Cmd, Event};

/// After the user changes something, status pushes still show the old value for a
/// moment. Leave the controls alone for this long.
const HOLD: Duration = Duration::from_millis(2500);
/// Sliders and spin buttons send once the value stops moving.
const DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Default)]
struct State {
    status: Option<Status>,
    led: Option<LedSettings>,
    shutdown: Option<ShutdownInfo>,
    smart_preset: Option<Preset>,
    /// Effect we last asked the LED settings for. Not cleared by the reply, so a
    /// device whose `51 00` disagrees with its status is asked once, not every second.
    led_requested: Option<u8>,
}

pub struct Ui {
    tx: Sender<Cmd>,
    pub window: adw::ApplicationWindow,
    updating: Cell<bool>,
    hold_until: Cell<Instant>,
    state: RefCell<State>,
    timers: RefCell<Vec<(&'static str, glib::SourceId)>>,
    connect_args: (Option<String>, bool),

    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    idle_page: adw::StatusPage,
    busy_label: gtk::Label,
    disconnect_btn: gtk::Button,

    temp_label: gtk::Label,
    temp_sub: gtk::Label,
    chart: Chart,
    fan_row: adw::ActionRow,
    hot_row: adw::ActionRow,
    output_row: adw::ActionRow,

    mode_group: adw::ToggleGroup,
    preset_row: adw::ActionRow,
    preset_group: adw::ToggleGroup,
    curve_group: adw::PreferencesGroup,
    curve_scales: Vec<gtk::Scale>,
    quick_group: adw::ToggleGroup,
    oc_row: adw::SwitchRow,

    light_row: adw::SwitchRow,
    effect_row: adw::ComboRow,
    speed_row: adw::ComboRow,
    brightness_row: adw::ActionRow,
    brightness: gtk::Scale,
    colors_row: adw::ActionRow,
    colors_box: gtk::Box,
    add_color: gtk::Button,
    remove_color: gtk::Button,
    rhythm_row: adw::SwitchRow,

    shutdown_row: adw::SwitchRow,
    minutes_row: adw::SpinRow,
    repeat_row: adw::SwitchRow,

    address_row: adw::ActionRow,
    firmware_row: adw::ActionRow,
    mac_row: adw::ActionRow,
}

fn mode_toggles(group: &adw::ToggleGroup) {
    for m in Mode::ALL {
        let label = match m {
            Mode::Smart => "Smart",
            Mode::Fixed => "Fixed",
            Mode::Custom => "Custom",
        };
        group.add(adw::Toggle::builder().name(m.name()).label(label).build());
    }
}

fn preset_label(p: Preset) -> &'static str {
    match p {
        Preset::Max => "Max",
        Preset::Silent => "Silent",
        Preset::Energy => "Energy saving",
        Preset::Strong => "Strong",
    }
}

fn value_label() -> gtk::Label {
    gtk::Label::builder().css_classes(["dim-label", "numeric"]).build()
}

fn row_with_value(title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&value_label());
    row
}

fn set_row_value(row: &adw::ActionRow, text: &str) {
    // The suffix label is the row's only suffix; find it through the widget tree.
    if let Some(label) = find_label(row.upcast_ref()) {
        label.set_label(text);
    }
}

fn find_label(w: &gtk::Widget) -> Option<gtk::Label> {
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(l) = c.downcast_ref::<gtk::Label>() {
            if l.has_css_class("numeric") {
                return Some(l.clone());
            }
        }
        if let Some(l) = find_label(&c) {
            return Some(l);
        }
        child = c.next_sibling();
    }
    None
}

fn hm(minutes: u16) -> String {
    format!("{}h {:02}m", minutes / 60, minutes % 60)
}

fn rgba(c: &p::Rgb) -> gdk::RGBA {
    gdk::RGBA::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0)
}

fn rgb(c: &gdk::RGBA) -> p::Rgb {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [b(c.red()), b(c.green()), b(c.blue())]
}

impl Ui {
    pub fn new(app: &adw::Application, tx: Sender<Cmd>, connect_args: (Option<String>, bool)) -> Rc<Ui> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("B9X")
            .default_width(440)
            .default_height(820)
            .build();

        // --- idle / busy pages ---
        let connect_btn = gtk::Button::builder()
            .label("Connect")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        let idle_page = adw::StatusPage::builder()
            .icon_name("bluetooth-symbolic")
            .title("Not connected")
            .description("Turn the cooler on and close the Flydigi app on your phone. The cooler accepts one connection at a time.")
            .child(&connect_btn)
            .build();
        let busy_label =
            gtk::Label::builder().css_classes(["title-3"]).wrap(true).justify(gtk::Justification::Center).build();
        let busy =
            gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).valign(gtk::Align::Center).build();
        let spinner = adw::Spinner::builder().width_request(48).height_request(48).build();
        let cancel_btn =
            gtk::Button::builder().label("Cancel").halign(gtk::Align::Center).css_classes(["pill"]).build();
        busy.append(&spinner);
        busy.append(&busy_label);
        busy.append(&cancel_btn);

        // --- status ---
        let temp_label = gtk::Label::builder().css_classes(["title-1", "numeric"]).label("–").build();
        let temp_sub = gtk::Label::builder().css_classes(["dim-label"]).label("cold plate").build();
        let chart = Chart::new();
        let head = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        head.append(&temp_label);
        head.append(&temp_sub);
        head.append(&chart.area);
        let head_frame =
            gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
        head_frame.append(&gtk::ListBoxRow::builder().child(&head).activatable(false).build());
        let fan_row = row_with_value("Fan");
        let hot_row = row_with_value("Hot side (estimated)");
        let output_row = row_with_value("Cooling output");
        let status_group = adw::PreferencesGroup::new();
        status_group.add(&head_frame);
        let readings = adw::PreferencesGroup::new();
        readings.add(&fan_row);
        readings.add(&hot_row);
        readings.add(&output_row);

        // --- cooling ---
        let mode_group = adw::ToggleGroup::builder().valign(gtk::Align::Center).build();
        mode_toggles(&mode_group);
        let mode_row = adw::ActionRow::builder().title("Mode").build();
        mode_row.add_suffix(&mode_group);
        let preset_group = adw::ToggleGroup::builder().valign(gtk::Align::Center).build();
        let preset_row = adw::ActionRow::builder().title("Preset").build();
        preset_row.add_suffix(&preset_group);
        let quick_group = adw::ToggleGroup::builder().valign(gtk::Align::Center).build();
        mode_toggles(&quick_group);
        let quick_row = adw::ActionRow::builder()
            .title("Quick switch")
            .subtitle("Long press on the power button toggles to this mode")
            .build();
        quick_row.add_suffix(&quick_group);
        let oc_row =
            adw::SwitchRow::builder().title("OC Experience").subtitle("Full power, frost protection off").build();
        let cooling = adw::PreferencesGroup::builder().title("Cooling").build();
        cooling.add(&mode_row);
        cooling.add(&preset_row);
        cooling.add(&quick_row);
        cooling.add(&oc_row);

        let curve_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .homogeneous(true)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(6)
            .margin_end(6)
            .build();
        let mut curve_scales = Vec::new();
        for t in p::CURVE_TEMPS {
            let scale =
                gtk::Scale::with_range(gtk::Orientation::Vertical, p::CURVE_MIN as f64, p::CURVE_MAX as f64, 1.0);
            scale.set_inverted(true);
            scale.set_round_digits(0);
            scale.set_digits(0);
            scale.set_draw_value(true);
            scale.set_value_pos(gtk::PositionType::Top);
            scale.set_height_request(180);
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
            col.append(&scale);
            col.append(&gtk::Label::builder().label(format!("{t}°")).css_classes(["caption", "dim-label"]).build());
            curve_box.append(&col);
            curve_scales.push(scale);
        }
        let curve_frame =
            gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
        curve_frame.append(&gtk::ListBoxRow::builder().child(&curve_box).activatable(false).build());
        let curve_group = adw::PreferencesGroup::builder()
            .title("Cooling curve")
            .description("Cooling level 1–8 for each cold plate temperature")
            .build();
        curve_group.add(&curve_frame);

        // --- lighting ---
        let light_row = adw::SwitchRow::builder().title("Light").build();
        let labels: Vec<&str> = p::EFFECTS.iter().map(|e| e.label).collect();
        let effect_row = adw::ComboRow::builder()
            .title("Effect")
            .subtitle("Stored per cooling mode")
            .model(&gtk::StringList::new(&labels))
            .build();
        let speed_row =
            adw::ComboRow::builder().title("Speed").model(&gtk::StringList::new(&["Slow", "Medium", "Fast"])).build();
        let brightness = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            p::BRIGHTNESS_MIN as f64,
            p::BRIGHTNESS_MAX as f64,
            1.0,
        );
        brightness.set_hexpand(true);
        brightness.set_round_digits(0);
        brightness.set_width_request(180);
        let brightness_row = adw::ActionRow::builder().title("Brightness").build();
        brightness_row.add_suffix(&brightness);
        let colors_box = gtk::Box::builder().spacing(6).valign(gtk::Align::Center).build();
        let add_color = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("Add colour")
            .css_classes(["flat"])
            .build();
        let remove_color = gtk::Button::builder()
            .icon_name("list-remove-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("Remove last colour")
            .css_classes(["flat"])
            .build();
        let colors_row = adw::ActionRow::builder().title("Colours").build();
        let colors_scroll = gtk::ScrolledWindow::builder()
            .child(&colors_box)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .hexpand(true)
            .build();
        colors_row.add_suffix(&colors_scroll);
        colors_row.add_suffix(&remove_color);
        colors_row.add_suffix(&add_color);
        let rhythm_row =
            adw::SwitchRow::builder().title("Colourful").subtitle("Rhythm: cycle colours instead of one").build();
        let lighting = adw::PreferencesGroup::builder().title("Lighting").build();
        lighting.add(&light_row);
        lighting.add(&effect_row);
        lighting.add(&speed_row);
        lighting.add(&brightness_row);
        lighting.add(&colors_row);
        lighting.add(&rhythm_row);

        // --- shutdown ---
        let shutdown_row = adw::SwitchRow::builder().title("Scheduled shutdown").build();
        let minutes_row = adw::SpinRow::with_range(1.0, p::SHUTDOWN_MAX_MIN as f64, 5.0);
        minutes_row.set_title("Minutes");
        minutes_row.set_subtitle(&hm(60));
        minutes_row.set_value(60.0);
        let repeat_row = adw::SwitchRow::builder().title("Repeat").subtitle("Every time instead of once").build();
        let shutdown = adw::PreferencesGroup::builder().title("Shutdown").build();
        shutdown.add(&shutdown_row);
        shutdown.add(&minutes_row);
        shutdown.add(&repeat_row);

        // --- device ---
        let address_row = row_with_value("Address");
        let firmware_row = row_with_value("Firmware");
        let mac_row = row_with_value("MAC");
        let device = adw::PreferencesGroup::builder().title("Device").build();
        device.add(&address_row);
        device.add(&firmware_row);
        device.add(&mac_row);

        let page = adw::PreferencesPage::new();
        for g in [&status_group, &readings, &cooling, &curve_group, &lighting, &shutdown, &device] {
            page.add(g);
        }

        let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).build();
        stack.add_named(&idle_page, Some("idle"));
        stack.add_named(&busy, Some("busy"));
        stack.add_named(&page, Some("main"));

        let disconnect_btn = gtk::Button::builder()
            .icon_name("network-offline-symbolic")
            .tooltip_text("Disconnect")
            .visible(false)
            .build();
        let header = adw::HeaderBar::new();
        header.pack_end(&disconnect_btn);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&toasts));
        window.set_content(Some(&view));

        let ui = Rc::new(Ui {
            tx,
            window,
            updating: Cell::new(false),
            hold_until: Cell::new(Instant::now()),
            state: RefCell::default(),
            timers: RefCell::default(),
            connect_args,
            toasts,
            stack,
            idle_page,
            busy_label,
            disconnect_btn,
            temp_label,
            temp_sub,
            chart,
            fan_row,
            hot_row,
            output_row,
            mode_group,
            preset_row,
            preset_group,
            curve_group,
            curve_scales,
            quick_group,
            oc_row,
            light_row,
            effect_row,
            speed_row,
            brightness_row,
            brightness,
            colors_row,
            colors_box,
            add_color,
            remove_color,
            rhythm_row,
            shutdown_row,
            minutes_row,
            repeat_row,
            address_row,
            firmware_row,
            mac_row,
        });
        ui.connect_signals(&connect_btn, &cancel_btn);
        ui
    }

    /// A signal handler that holds the UI weakly, so widgets do not keep it alive.
    fn handler(self: &Rc<Self>, f: impl Fn(&Rc<Ui>) + 'static) -> impl Fn() + 'static {
        let weak = Rc::downgrade(self);
        move || {
            if let Some(ui) = weak.upgrade() {
                f(&ui);
            }
        }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.try_send(cmd);
    }

    /// For handlers: true while widgets are being set from device data.
    fn user(&self) -> bool {
        if self.updating.get() {
            return false;
        }
        self.hold_until.set(Instant::now() + HOLD);
        true
    }

    fn held(&self) -> bool {
        Instant::now() < self.hold_until.get()
    }

    fn quiet(&self, f: impl FnOnce()) {
        let was = self.updating.replace(true);
        f();
        self.updating.set(was);
    }

    /// Run `f` once `key` has not been touched for [`DEBOUNCE`].
    fn debounce(self: &Rc<Self>, key: &'static str, f: impl FnOnce(&Ui) + 'static) {
        self.cancel(key);
        let mut timers = self.timers.borrow_mut();
        let ui = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(DEBOUNCE, move || {
            if let Some(ui) = ui.upgrade() {
                ui.timers.borrow_mut().retain(|(k, _)| *k != key);
                f(&ui);
            }
        });
        timers.push((key, id));
    }

    /// Drop a debounced change that has not gone out yet.
    fn cancel(&self, key: &str) {
        let mut timers = self.timers.borrow_mut();
        if let Some(i) = timers.iter().position(|(k, _)| *k == key) {
            timers.remove(i).1.remove();
        }
    }

    fn pending(&self, key: &str) -> bool {
        self.timers.borrow().iter().any(|(k, _)| *k == key)
    }

    pub fn connect(&self) {
        let (address, fake) = self.connect_args.clone();
        self.busy_label.set_label("Connecting…");
        self.stack.set_visible_child_name("busy");
        self.send(Cmd::Connect { address, fake });
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::builder().title(glib::markup_escape_text(msg)).timeout(5).build());
    }

    fn connect_signals(self: &Rc<Self>, connect_btn: &gtk::Button, cancel_btn: &gtk::Button) {
        let c = self.handler(|ui| ui.connect());
        connect_btn.connect_clicked(move |_| c());
        let c = self.handler(|ui| ui.send(Cmd::Disconnect));
        cancel_btn.connect_clicked(move |_| c());
        let c = self.handler(|ui| ui.send(Cmd::Disconnect));
        self.disconnect_btn.connect_clicked(move |_| c());

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            let Some(mode) = ui.mode_group.active_name().and_then(|n| Mode::from_name(&n)) else { return };
            let preset = match mode {
                Mode::Smart => Some(ui.state.borrow().smart_preset.unwrap_or(Preset::Silent)),
                _ => None,
            };
            // A curve frame would switch straight back to Custom.
            ui.cancel("curve");
            ui.show_mode(mode);
            ui.send(Cmd::Mode(mode, preset));
        });
        self.mode_group.connect_active_name_notify(move |_| c());

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            let mode = ui.mode_group.active_name().and_then(|n| Mode::from_name(&n));
            let preset = ui.preset_group.active_name().and_then(|n| Preset::from_name(&n));
            if let (Some(m), Some(pr)) = (mode, preset) {
                ui.send(Cmd::Mode(m, Some(pr)));
            }
        });
        self.preset_group.connect_active_name_notify(move |_| c());

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            if let Some(m) = ui.quick_group.active_name().and_then(|n| Mode::from_name(&n)) {
                ui.send(Cmd::QuickSwitch(m));
            }
        });
        self.quick_group.connect_active_name_notify(move |_| c());

        for scale in &self.curve_scales {
            let c = self.handler(|ui| {
                if !ui.user() {
                    return;
                }
                ui.debounce("curve", |ui| {
                    if ui.mode_group.active_name().as_deref() != Some(Mode::Custom.name()) {
                        return;
                    }
                    let levels = ui.curve_scales.iter().map(|s| s.value().round() as u8).collect();
                    ui.send(Cmd::Curve(levels));
                });
            });
            scale.connect_value_changed(move |_| c());
        }

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            if !ui.oc_row.is_active() {
                ui.send(Cmd::Overclock(false));
                return;
            }
            // Ask first; the switch only turns on once the device reports it.
            ui.quiet(|| ui.oc_row.set_active(false));
            let dialog = adw::AlertDialog::new(Some("Turn on OC Experience?"), Some(OC_WARNING));
            dialog.add_responses(&[("cancel", "Cancel"), ("on", "Turn On")]);
            dialog.set_response_appearance("on", adw::ResponseAppearance::Destructive);
            dialog.set_close_response("cancel");
            let weak = Rc::downgrade(ui);
            dialog.choose(Some(&ui.window), None::<&gtk::gio::Cancellable>, move |r| {
                if let (Some(ui), "on") = (weak.upgrade(), r.as_str()) {
                    ui.hold_until.set(Instant::now() + HOLD);
                    ui.quiet(|| ui.oc_row.set_active(true));
                    ui.send(Cmd::Overclock(true));
                }
            });
        });
        self.oc_row.connect_active_notify(move |_| c());

        let c = self.handler(|ui| {
            if ui.user() {
                ui.send(Cmd::Light(ui.light_row.is_active()));
            }
        });
        self.light_row.connect_active_notify(move |_| c());

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            let Some(e) = p::EFFECTS.get(ui.effect_row.selected() as usize) else { return };
            // A pending brightness belongs to the old effect; sending it would select that again.
            ui.cancel("brightness");
            ui.state.borrow_mut().led_requested = Some(e.code);
            ui.send(Cmd::Led(e.code, LedChange::default()));
        });
        self.effect_row.connect_selected_notify(move |_| c());

        let c = self.handler(|ui| {
            if ui.user() {
                let speed = ui.speed_row.selected() as usize;
                ui.send_led(LedChange { speed: Some(speed), ..Default::default() });
            }
        });
        self.speed_row.connect_selected_notify(move |_| c());

        let c = self.handler(|ui| {
            if !ui.user() {
                return;
            }
            ui.debounce("brightness", |ui| {
                let b = ui.brightness.value().round() as u8;
                ui.send_led(LedChange { brightness: Some(b), ..Default::default() });
            });
        });
        self.brightness.connect_value_changed(move |_| c());

        let c = self.handler(|ui| {
            if ui.user() {
                ui.send_led(LedChange { rhythm_colorful: Some(ui.rhythm_row.is_active()), ..Default::default() });
            }
        });
        self.rhythm_row.connect_active_notify(move |_| c());

        let c = self.handler(|ui| ui.change_colors(1));
        self.add_color.connect_clicked(move |_| c());
        let c = self.handler(|ui| ui.change_colors(-1));
        self.remove_color.connect_clicked(move |_| c());

        let c = self.handler(|ui| {
            if ui.user() {
                ui.send_shutdown();
            }
        });
        self.shutdown_row.connect_active_notify(move |_| c());
        let c = self.handler(|ui| {
            ui.minutes_row.set_subtitle(&hm(ui.minutes_row.value() as u16));
            if ui.user() && ui.shutdown_row.is_active() {
                ui.debounce("shutdown", |ui| ui.send_shutdown());
            }
        });
        self.minutes_row.connect_value_notify(move |_| c());
        let c = self.handler(|ui| {
            if ui.user() && ui.shutdown_row.is_active() {
                ui.send_shutdown();
            }
        });
        self.repeat_row.connect_active_notify(move |_| c());
    }

    /// Change settings of the effect shown in the window, which is the one the user
    /// picked last even before the device has answered.
    fn send_led(&self, change: LedChange) {
        if let Some(e) = p::EFFECTS.get(self.effect_row.selected() as usize) {
            self.send(Cmd::Led(e.code, change));
        }
    }

    fn send_shutdown(&self) {
        let schedule =
            self.shutdown_row.is_active().then(|| (self.minutes_row.value() as u16, self.repeat_row.is_active()));
        self.send(Cmd::Shutdown(schedule));
    }

    fn color_buttons(&self) -> Vec<gtk::ColorDialogButton> {
        let mut buttons = Vec::new();
        let mut child = self.colors_box.first_child();
        while let Some(c) = child {
            if let Some(b) = c.downcast_ref::<gtk::ColorDialogButton>() {
                buttons.push(b.clone());
            }
            child = c.next_sibling();
        }
        buttons
    }

    fn current_colors(&self) -> Vec<p::Rgb> {
        self.color_buttons().iter().map(|b| rgb(&b.rgba())).collect()
    }

    fn change_colors(&self, delta: i32) {
        if !self.user() {
            return;
        }
        let mut colors = self.current_colors();
        if delta > 0 {
            colors.push(colors.last().copied().unwrap_or([0xff, 0xff, 0xff]));
        } else {
            colors.pop();
        }
        self.send_led(LedChange { colors: Some(colors), ..Default::default() });
    }

    fn show_mode(&self, mode: Mode) {
        let custom = mode == Mode::Custom;
        self.preset_row.set_visible(!custom);
        self.curve_group.set_visible(custom);
        let presets: Vec<Preset> = p::presets_for(mode).collect();
        let current: Vec<String> = (0..self.preset_group.n_toggles())
            .filter_map(|i| self.preset_group.toggle(i).map(|t| t.name().to_string()))
            .collect();
        if current != presets.iter().map(|p| p.name().to_string()).collect::<Vec<_>>() {
            self.quiet(|| {
                self.preset_group.remove_all();
                for pr in presets {
                    self.preset_group.add(adw::Toggle::builder().name(pr.name()).label(preset_label(pr)).build());
                }
            });
        }
    }

    pub fn handle(self: &Rc<Self>, ev: Event) {
        match ev {
            Event::Connecting(msg) => {
                self.busy_label.set_label(&msg);
                self.stack.set_visible_child_name("busy");
            }
            Event::Connected { address, version, mac } => {
                set_row_value(&self.address_row, &address);
                set_row_value(&self.firmware_row, version.as_deref().unwrap_or("not reported"));
                set_row_value(&self.mac_row, &mac);
                self.chart.clear();
                self.disconnect_btn.set_visible(true);
                self.stack.set_visible_child_name("main");
            }
            Event::Disconnected(why) => {
                *self.state.borrow_mut() = State::default();
                self.disconnect_btn.set_visible(false);
                self.temp_label.set_label("–");
                self.stack.set_visible_child_name("idle");
                if let Some(why) = why {
                    self.idle_page.set_title("Connection lost");
                    self.toast(&why);
                } else {
                    self.idle_page.set_title("Not connected");
                }
            }
            Event::Error(msg) => {
                self.toast(&msg);
                // Show what the device really has instead of the rejected change.
                self.hold_until.set(Instant::now());
                if let Some(led) = self.state.borrow().led.clone() {
                    self.show_led(&led);
                }
            }
            Event::Quit => {
                if let Some(app) = self.window.application() {
                    app.quit();
                }
            }
            Event::Status(s) => self.show_status(s),
            Event::Led(l) => self.show_led(&l),
            Event::Curve(levels) => self.quiet(|| {
                for (scale, v) in self.curve_scales.iter().zip(levels) {
                    scale.set_value(v as f64);
                }
            }),
            Event::Shutdown(sd) => {
                self.state.borrow_mut().shutdown = Some(sd);
                self.quiet(|| {
                    self.shutdown_row.set_active(sd.enable);
                    self.repeat_row.set_active(sd.repeat);
                    if sd.minutes > 0 {
                        self.minutes_row.set_value(sd.minutes as f64);
                    }
                });
            }
        }
    }

    fn show_status(self: &Rc<Self>, s: Status) {
        self.temp_label.set_label(&format!("{:.1} °C", s.cold_temp));
        self.chart.push(s.cold_temp);
        set_row_value(&self.fan_row, &format!("{} rpm", s.fan_rpm));
        set_row_value(&self.hot_row, &format!("{} °C", s.hot_temp));
        set_row_value(&self.output_row, &s.output.to_string());
        let sub = match (s.overclock, s.shutdown_left) {
            (true, _) => "cold plate · OC on".to_string(),
            (false, 0) => "cold plate".to_string(),
            (false, left) => format!("cold plate · off in {}", hm(left)),
        };
        self.temp_sub.set_label(&sub);
        let left = match s.shutdown_left {
            0 => String::new(),
            m => format!("Turns off in {}", hm(m)),
        };
        self.shutdown_row.set_subtitle(&left);

        let mode = Mode::from_code(s.mode);
        if mode == Some(Mode::Smart) {
            self.state.borrow_mut().smart_preset = s.preset();
        }
        if !self.held() {
            self.quiet(|| {
                if let Some(m) = mode {
                    self.show_mode(m);
                    self.mode_group.set_active_name(Some(m.name()));
                }
                self.preset_group.set_active_name(s.preset().map(Preset::name));
                for m in Mode::ALL {
                    if let Some(t) = self.quick_group.toggle_by_name(m.name()) {
                        t.set_enabled(Some(m) != mode);
                    }
                }
                self.quick_group.set_active_name(Mode::from_code(s.quick_switch).map(Mode::name));
                self.oc_row.set_active(s.overclock);
                self.light_row.set_active(s.light);
                if s.shutdown_left == 0 && self.shutdown_row.is_active() {
                    self.shutdown_row.set_active(false);
                }
            });
        }

        // The effect changes with the mode or a double press on the power button.
        let mut st = self.state.borrow_mut();
        let known = st.led.as_ref().map(|l| l.effect);
        if known != Some(s.effect) && st.led_requested != Some(s.effect) {
            st.led_requested = Some(s.effect);
            self.send(Cmd::RefreshLed);
        }
        st.status = Some(s);
    }

    fn show_led(self: &Rc<Self>, l: &LedSettings) {
        self.state.borrow_mut().led = Some(l.clone());
        let Some(e) = p::effect_by_code(l.effect) else { return };
        self.quiet(|| {
            if let Some(i) = p::EFFECTS.iter().position(|x| x.code == e.code) {
                self.effect_row.set_selected(i as u32);
            }
            self.speed_row.set_visible(e.writable && e.speeds.is_some());
            if let Some(name) = l.speed_name() {
                self.speed_row.set_selected(p::SPEED_NAMES.iter().position(|n| *n == name).unwrap_or(1) as u32);
            }
            self.brightness_row.set_visible(e.writable);
            // Leave the slider alone while a drag is still waiting to go out.
            if l.brightness >= p::BRIGHTNESS_MIN && !self.pending("brightness") {
                self.brightness.set_value(l.brightness as f64);
            }
            self.rhythm_row.set_visible(e.name == "rhythm");
            self.rhythm_row.set_active(l.color_mode == p::RHYTHM_COLORFUL);

            self.colors_row.set_visible(e.colors.is_some());
            if let Some((lo, hi)) = e.colors {
                let buttons = self.color_buttons();
                if buttons.len() == l.colors.len() {
                    // Update in place: rebuilding would close an open colour dialog.
                    for (b, c) in buttons.iter().zip(&l.colors) {
                        if rgb(&b.rgba()) != *c {
                            b.set_rgba(&rgba(c));
                        }
                    }
                } else {
                    while let Some(c) = self.colors_box.first_child() {
                        self.colors_box.remove(&c);
                    }
                    for c in &l.colors {
                        self.colors_box.append(&self.color_button(c));
                    }
                }
                self.add_color.set_visible(hi > 1);
                self.remove_color.set_visible(hi > 1);
                self.add_color.set_sensitive(l.colors.len() < hi);
                self.remove_color.set_sensitive(l.colors.len() > lo);
            }
        });
    }

    fn color_button(self: &Rc<Self>, c: &p::Rgb) -> gtk::ColorDialogButton {
        let dialog = gtk::ColorDialog::builder().with_alpha(false).build();
        let button = gtk::ColorDialogButton::new(Some(dialog));
        button.set_rgba(&rgba(c));
        button.set_valign(gtk::Align::Center);
        let weak = Rc::downgrade(self);
        button.connect_rgba_notify(move |_| {
            if let Some(ui) = weak.upgrade() {
                if ui.user() {
                    ui.send_led(LedChange { colors: Some(ui.current_colors()), ..Default::default() });
                }
            }
        });
        button
    }
}
