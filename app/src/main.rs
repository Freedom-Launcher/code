#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{sync::Arc, time::Instant};

use fl_core::{Caller, Core};
use serde_json::Value;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

struct AppCore(Arc<Core>);

/// The single bridge between the UI and the core: every UI feature is an action.
#[tauri::command]
async fn action(
    state: tauri::State<'_, AppCore>,
    name: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let core = Arc::clone(&state.0);
    let out = core
        .dispatch(&Caller::BuiltinUi, &name, args.unwrap_or(Value::Null))
        .await
        .map_err(|e| e.to_string());
    if name == "app.ready" && std::env::var_os("FREEDOM_LOG_STARTUP").is_some() {
        eprintln!("[freedom] ready: {:?}", out);
    }
    out
}

/// Opens the Microsoft sign-in page in the system browser. Only fixed Microsoft URLs are allowed.
#[tauri::command]
fn open_login_page(app: tauri::AppHandle, url: String) -> Result<(), String> {
    const ALLOWED: [&str; 2] = [
        "https://www.microsoft.com/link",
        "https://microsoft.com/devicelogin",
    ];
    let ok = ALLOWED
        .iter()
        .any(|a| url == *a || url.starts_with(&format!("{a}?")));
    if !ok {
        return Err("this address is not allowed".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens an instance folder in the system file manager.
#[tauri::command]
async fn open_instance_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    instance: String,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(
            &Caller::BuiltinUi,
            "instances.folder",
            serde_json::json!({ "instance": instance }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no folder")?.to_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Serves files of installed themes: `theme://localhost/<id>/<path>` (Windows:
/// `http://theme.localhost/<id>/<path>`). Only checked asset types inside the theme
/// folder; CSS is re-checked on every request, so editing a file cannot sneak code in.
fn theme_protocol(
    app: &tauri::AppHandle,
    req: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use fl_core::themes;
    use tauri::http::Response;
    let deny = |code: u16| Response::builder().status(code).body(Vec::new()).unwrap();
    let Some(state) = app.try_state::<AppCore>() else {
        return deny(503);
    };
    let core = &state.0;
    if core.safe_mode {
        return deny(403);
    }
    let path = req.uri().path().trim_start_matches('/');
    let Some((id, rel)) = path.split_once('/') else {
        return deny(404);
    };
    if !themes::safe_rel(rel, &themes::ASSETS) {
        return deny(403);
    }
    let Some(file) = themes::file(&core.data_dir, id, rel) else {
        return deny(404);
    };
    let Ok(bytes) = std::fs::read(&file) else {
        return deny(404);
    };
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if ext == "css" {
        let ok = std::str::from_utf8(&bytes).is_ok_and(|s| themes::check_css(s).is_ok());
        if !ok {
            return deny(403);
        }
    }
    let mime = match ext.as_str() {
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => return deny(403),
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        // SVGs are images only: no script even if opened directly.
        .header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; img-src data:")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
}

/// Opens the themes folder in the system file manager.
#[tauri::command]
async fn open_themes_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(&Caller::BuiltinUi, "themes.folder", Value::Null)
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no folder")?.to_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Shift held while the launcher starts opens safe settings.
#[cfg(windows)]
fn shift_held() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
    // SAFETY: plain Win32 query with no pointers.
    unsafe { (GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0 }
}
#[cfg(not(windows))]
fn shift_held() -> bool {
    false
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Starts a separate copy in safe settings and quits this one.
fn restart_safe(app: &tauri::AppHandle) {
    if let Ok(exe) = std::env::current_exe() {
        if std::process::Command::new(exe)
            .arg("--safe")
            .spawn()
            .is_ok()
        {
            app.exit(0);
        }
    }
}

fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let safe = MenuItem::with_id(app, "safe", "Безопасные настройки", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &safe, &sep, &quit])?;
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("Freedom Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "safe" => restart_safe(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(t.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

fn main() {
    let started = Instant::now();
    let safe_mode = std::env::args().any(|a| a == "--safe") || shift_held();

    let mut builder = tauri::Builder::default();
    // Safe settings may run next to a stuck copy, so only the normal mode is single-instance.
    if !safe_mode {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }));
    }
    builder
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("theme", |ctx, req| theme_protocol(ctx.app_handle(), &req))
        .setup(move |app| {
            tray(app)?;
            let data_dir = app.path().app_data_dir()?;
            let core = Core::new(data_dir, started, safe_mode)?;
            app.manage(AppCore(core));
            if safe_mode {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_title("Freedom Launcher — безопасные настройки");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            action,
            open_login_page,
            open_instance_folder,
            open_themes_folder
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Freedom Launcher");
}
