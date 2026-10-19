//! GTK 4 / libadwaita front end for the Flydigi B9X (Linux).
//!
//! Usage: b9x-gui [-a ADDRESS] [--fake]. ADDRESS defaults to $B9X_ADDRESS, then a scan.

mod chart;
mod window;
mod worker;

use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use tokio::sync::oneshot;

use crate::worker::Cmd;

const APP_ID: &str = "io.github.flydigib9x.B9X";

fn parse_args() -> (Option<String>, bool) {
    let mut address = std::env::var("B9X_ADDRESS").ok().filter(|a| !a.is_empty());
    let mut fake = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--fake" => fake = true,
            "-a" | "--address" => address = args.next(),
            "-h" | "--help" => {
                println!("usage: b9x-gui [-a ADDRESS] [--fake]");
                std::process::exit(0);
            }
            _ => {
                eprintln!("b9x-gui: unknown argument {a}");
                std::process::exit(2);
            }
        }
    }
    (address, fake)
}

fn main() -> glib::ExitCode {
    let connect_args = parse_args();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let (cmd_tx, cmd_rx) = async_channel::unbounded();
    let (ev_tx, ev_rx) = async_channel::unbounded();
    // Quit through the normal path on signals so the link gets closed below.
    runtime.spawn(worker::quit_on_signal(ev_tx.clone()));
    runtime.spawn(worker::run(cmd_rx, ev_tx));

    let app = adw::Application::builder().application_id(APP_ID).build();
    let tx = cmd_tx.clone();
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let ui = window::Ui::new(app, tx.clone(), connect_args.clone());
        let events = ev_rx.clone();
        let handler = ui.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = events.recv().await {
                handler.handle(ev);
            }
        });
        ui.window.present();
        ui.connect();
    });

    // GTK arguments are ours; do not let GApplication parse them.
    let code = app.run_with_args::<&str>(&[]);

    // BlueZ keeps the link up after we exit, which locks out the phone app.
    let (done_tx, done_rx) = oneshot::channel();
    if cmd_tx.try_send(Cmd::Quit(done_tx)).is_ok() {
        runtime.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(5), done_rx).await;
        });
    }
    code
}
