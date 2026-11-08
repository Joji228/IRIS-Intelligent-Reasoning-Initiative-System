//! I.R.I.S. desktop shell: one window, a tray icon, start-with-Windows, and the
//! bundled engine kept alive in the background.

mod engine;

use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Manager, RunEvent, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::engine::{ENGINE_PORT, Supervisor};

/// Passed by the Windows autostart entry: start hidden in the tray.
const MINIMIZED_ARG: &str = "--minimized";
/// `iris-desktop --quit` asks a running I.R.I.S. to exit cleanly (stopping
/// the engine and removing the tray icon); the installer uses it.
const QUIT_ARG: &str = "--quit";

#[derive(Serialize)]
struct Connection {
    url: String,
    token: String,
}

/// Where the UI finds the engine. Waits (up to ~20 s) for the engine to come up
/// and write its local token, which takes a moment on first start.
#[tauri::command]
async fn connection() -> Result<Connection, String> {
    let token_file = engine::data_dir().join("local-token.txt");
    let mut impostor = false;
    for _ in 0..40 {
        if engine::engine_reachable() {
            if let Ok(token) = std::fs::read_to_string(&token_file) {
                let token = token.trim().to_owned();
                // Only the real engine can prove it knows the token; anything
                // else on the port (another program, another account) gets nothing.
                if engine::engine_knows_token(&token) {
                    return Ok(Connection { url: format!("http://127.0.0.1:{ENGINE_PORT}"), token });
                }
                impostor = true;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(if impostor {
        format!(
            "something other than the I.R.I.S. engine is answering on port {ENGINE_PORT}, so the app did not connect (it would have had to hand it your access token). Close the program using that port, or restart your computer."
        )
    } else {
        "the I.R.I.S. engine did not start".into()
    })
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn quit(app: &AppHandle) {
    app.state::<Supervisor>().stop();
    app.exit(0);
}

/// True once Windows' taskbar window exists (a tray icon added before that is lost).
fn taskbar_ready() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
        let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a valid NUL-terminated UTF-16 class name and a null window name.
        !unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }.is_null()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// At login Windows can start us before the taskbar is ready, and an icon added
/// then silently never appears. Wait for the taskbar (plus a moment for the
/// notification area to settle) before creating the tray icon.
fn build_tray_when_taskbar_ready(app: AppHandle) {
    std::thread::spawn(move || {
        for _ in 0..240 {
            if taskbar_ready() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        std::thread::sleep(Duration::from_secs(5));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Err(e) = build_tray(&handle) {
                eprintln!("could not create the tray icon: {e}");
            }
        });
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open I.R.I.S.", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit I.R.I.S.", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit_item])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("I.R.I.S.")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let supervisor = Supervisor::default();

    tauri::Builder::default()
        // A second launch (e.g. from the Start menu) just brings this one forward.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == QUIT_ARG) {
                quit(app);
            } else {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![MINIMIZED_ARG])))
        .plugin(tauri_plugin_opener::init())
        .manage(supervisor.clone())
        .invoke_handler(tauri::generate_handler![connection])
        .setup(move |app| {
            // Nothing was running to quit: don't start one either.
            if std::env::args().any(|a| a == QUIT_ARG) {
                app.handle().exit(0);
                return Ok(());
            }
            supervisor.start();

            // Start with Windows by default; the app is meant to be always on.
            let autostart = app.autolaunch();
            if !autostart.is_enabled().unwrap_or(false) {
                let _ = autostart.enable();
            }

            // `--minimized` means we were started at login: stay hidden, and
            // add the tray icon only once the taskbar exists.
            if std::env::args().any(|a| a == MINIMIZED_ARG) {
                build_tray_when_taskbar_ready(app.handle().clone());
            } else {
                build_tray(app.handle())?;
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps I.R.I.S. running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the I.R.I.S. app")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<Supervisor>().stop();
            }
        });
}
