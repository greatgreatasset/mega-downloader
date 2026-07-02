// Hide the console window in release builds; keep it in debug for engine logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use tauri::{Manager, RunEvent};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// Holds the engine sidecar's child handle so it can be killed on app exit.
struct EngineProcess(Mutex<Option<CommandChild>>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Per-user locations: DB in the app-data dir, downloads in the
            // user's Downloads/MegaDownloader (falling back to app-data).
            let data_dir = handle.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("mega-downloader.db");

            let downloads = handle
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("MegaDownloader");
            std::fs::create_dir_all(&downloads).ok();

            // Spawn the headless engine as a sidecar, pointing it at those dirs.
            let (mut rx, child) = handle
                .shell()
                .sidecar("mega-downloader")?
                .env("DB_PATH", db_path.to_string_lossy().to_string())
                .env("DOWNLOAD_DIR", downloads.to_string_lossy().to_string())
                .spawn()?;

            app.manage(EngineProcess(Mutex::new(Some(child))));

            // Surface engine logs to the host process output.
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            println!("[engine] {}", String::from_utf8_lossy(&line));
                        }
                        CommandEvent::Stderr(line) => {
                            eprintln!("[engine] {}", String::from_utf8_lossy(&line));
                        }
                        _ => {}
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the application")
        .run(|app, event| {
            // Make sure the engine doesn't outlive the window.
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(proc) = app.try_state::<EngineProcess>() {
                    if let Some(child) = proc.0.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                }
            }
        });
}
