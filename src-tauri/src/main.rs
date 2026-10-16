//! The Transmute desktop app: a Tauri shell around `transmute-api`.
//!
//! The shell holds no logic. It opens one API session, forwards commands
//! to it (`commands`) and emits every API event to the webview on the
//! `transmute` channel.

// No console window behind the app in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use transmute_api::{Api, ApiEvent};

/// The one event channel the frontend listens on.
const EVENT_CHANNEL: &str = "transmute";

const SETTINGS_HINT: &str = r#"Transmute keeps its settings in %APPDATA%\Transmute, or in a "data" folder beside the program in portable mode. Check that it can be written to, then start Transmute again."#;

/// Says why the session couldn't open, then quits once the user has read
/// it. A release build has no console, so without this it would just vanish.
fn startup_failed(app: &tauri::AppHandle, reason: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    // The window would only show an app that can't do anything.
    for window in app.webview_windows().values() {
        let _ = window.hide();
    }
    let exit = app.clone();
    app.dialog()
        .message(format!("{reason}\n\n{SETTINGS_HINT}"))
        .title("Transmute couldn't start")
        .kind(MessageDialogKind::Error)
        .show(move |_| exit.exit(1));
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let sink = Arc::new(move |event: ApiEvent| {
                // Fails only while the app is shutting down.
                let _ = handle.emit(EVENT_CHANNEL, event);
            });
            // Opening probes the installed tools, which needs a Tokio
            // runtime: use Tauri's, so the session's tasks share it.
            match tauri::async_runtime::block_on(async { Api::open(sink) }) {
                Ok(api) => {
                    app.manage(api);
                }
                Err(err) => startup_failed(app.handle(), &err.message),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::dismiss_notices,
            commands::add_inputs,
            commands::remove_root,
            commands::clear_inputs,
            commands::tree,
            commands::file_counts,
            commands::files,
            commands::entry,
            commands::clear_finished,
            commands::set_excluded,
            commands::preset,
            commands::set_preset,
            commands::stored_presets,
            commands::load_preset,
            commands::save_preset,
            commands::delete_preset,
            commands::new_preset,
            commands::save_working,
            commands::import_preset,
            commands::export_preset,
            commands::codecs,
            commands::defaults,
            commands::set_defaults,
            commands::settings,
            commands::set_settings,
            commands::tools,
            commands::refresh_tools,
            commands::plan,
            commands::jobs,
            commands::start_run,
            commands::pause,
            commands::resume,
            commands::cancel,
            commands::report,
            commands::save_report,
            commands::run_totals,
            commands::reveal,
            commands::preview,
            commands::sample,
            commands::thumbnail,
            commands::clear_previews,
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(err) => {
            eprintln!("Transmute couldn't start: {err}");
            std::process::exit(1);
        }
    }
}
