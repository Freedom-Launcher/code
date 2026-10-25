#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{sync::Arc, time::Instant};

use fl_core::{Caller, Core};
use serde_json::Value;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

mod updater;

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

/// A `data-action` click inside markup written by a theme. Runs with theme rights:
/// only `bindable` actions, attributed to the active theme.
#[tauri::command]
async fn theme_action(
    state: tauri::State<'_, AppCore>,
    name: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let core = Arc::clone(&state.0);
    let cur = core
        .dispatch(&Caller::BuiltinUi, "themes.current", Value::Null)
        .await
        .map_err(|e| e.to_string())?;
    let id = cur["id"].as_str().unwrap_or("default").to_owned();
    core.dispatch(&Caller::Theme { id }, &name, args.unwrap_or(Value::Null))
        .await
        .map_err(|e| e.to_string())
}

/// Opens a link from a project page in the system browser: plain https only.
#[tauri::command]
fn open_link(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if !link_ok(&url) {
        return Err("this address is not allowed".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

fn link_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    url.len() <= 2048
        && !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
        && url
            .bytes()
            .all(|b| b > 0x20 && b < 0x7f && !matches!(b, b'"' | b'\\' | b'<' | b'>'))
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

type LoginTx = Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<Result<String, String>>>>>;

fn deliver_login(tx: &LoginTx, r: Result<String, String>) {
    if let Some(t) = tx.lock().ok().and_then(|mut g| g.take()) {
        let _ = t.send(r);
    }
}

/// Modrinth-style sign-in: the Microsoft page opens in a launcher window. The window stops at
/// the registered redirect address (never loading it) and hands the address to the core,
/// which exchanges the code. The device code flow stays available as a fallback.
#[tauri::command]
async fn ms_window_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
) -> Result<Value, String> {
    const LABEL: &str = "mslogin";
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Err("SIGNIN_WINDOW_OPEN".into());
    }
    let core = Arc::clone(&state.0);
    let start = core
        .dispatch(&Caller::BuiltinUi, "accounts.microsoft.authorize", Value::Null)
        .await
        .map_err(|e| e.to_string())?;
    let url: tauri::Url = start["url"]
        .as_str()
        .ok_or("no sign-in address")?
        .parse()
        .map_err(|e| format!("{e}"))?;

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let tx: LoginTx = Arc::new(std::sync::Mutex::new(Some(tx)));
    let nav_tx = Arc::clone(&tx);
    let w = tauri::WebviewWindowBuilder::new(&app, LABEL, tauri::WebviewUrl::External(url))
        .title(tr("Вход через Microsoft"))
        .inner_size(480.0, 640.0)
        .min_inner_size(380.0, 520.0)
        .center()
        .incognito(true)
        .on_navigation(move |u| {
            if u.as_str().starts_with(fl_core::msauth::REDIRECT_URI) {
                deliver_login(&nav_tx, Ok(u.to_string()));
                return false;
            }
            matches!(u.scheme(), "https" | "about")
        })
        .build()
        .map_err(|e| e.to_string())?;
    let close_tx = Arc::clone(&tx);
    w.on_window_event(move |e| {
        if matches!(
            e,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            deliver_login(&close_tx, Err("cancelled".into()));
        }
    });

    let res = tokio::time::timeout(std::time::Duration::from_secs(600), rx).await;
    let _ = w.destroy();
    let redirect = match res {
        Ok(Ok(Ok(u))) => u,
        Ok(Ok(Err(e))) => return Err(e),
        Ok(Err(_)) => return Err("cancelled".into()),
        Err(_) => return Err("sign-in took too long, start sign-in again".into()),
    };
    core.dispatch(
        &Caller::BuiltinUi,
        "accounts.microsoft.complete",
        serde_json::json!({ "url": redirect }),
    )
    .await
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

/// Highlights a content file in the system file manager («Показать файл»).
#[tauri::command]
async fn reveal_content(state: tauri::State<'_, AppCore>, args: Value) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(&Caller::BuiltinUi, "mods.path", args)
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no file")?.to_owned();
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// «Загрузить файлы»: native multi-file picker, then the core copies the files in.
#[tauri::command]
async fn pick_content(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    args: Value,
    ext: String,
) -> Result<Value, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(tr("Файлы"), &[ext.trim_start_matches('.')])
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    if picked.is_empty() {
        return Ok(serde_json::json!({ "added": [] }));
    }
    add_content(state, args, picked).await
}

/// Files dropped onto the content tab.
#[tauri::command]
async fn drop_content(state: tauri::State<'_, AppCore>, args: Value, paths: Vec<String>) -> Result<Value, String> {
    add_content(state, args, paths).await
}

async fn add_content(state: tauri::State<'_, AppCore>, mut args: Value, paths: Vec<String>) -> Result<Value, String> {
    args["paths"] = serde_json::json!(paths);
    let core = Arc::clone(&state.0);
    core.dispatch(&Caller::BuiltinUi, "mods.add_files", args)
        .await
        .map_err(|e| e.to_string())
}

/// Settings → Плагины → «Выбрать файл…»: install a plugin `.zip`.
#[tauri::command]
async fn pick_addon(app: tauri::AppHandle, state: tauri::State<'_, AppCore>) -> Result<Value, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(tr("Плагины"), &["zip"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked else { return Ok(Value::Null) };
    let core = Arc::clone(&state.0);
    core.dispatch(&Caller::BuiltinUi, "addons.install", serde_json::json!({ "path": path }))
        .await
        .map_err(|e| e.to_string())
}

/// Native folder picker (Settings → Игра → «Папка сборок»); `None` if cancelled.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file();
        if let Some(t) = title {
            d = d.set_title(t);
        }
        d.blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Opens one world's folder in the system file manager.
#[tauri::command]
async fn open_world_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    instance: String,
    world: String,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(
            &Caller::BuiltinUi,
            "worlds.folder",
            serde_json::json!({ "instance": instance, "world": world }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no folder")?.to_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens a folder an action returns as `path` (screenshots, logs).
async fn open_action_path(
    app: tauri::AppHandle,
    core: Arc<Core>,
    name: &str,
    args: Value,
) -> Result<(), String> {
    let v = core
        .dispatch(&Caller::BuiltinUi, name, args)
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no folder")?.to_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens an instance's `screenshots/` folder.
#[tauri::command]
async fn open_shots_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    instance: String,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    open_action_path(app, core, "screenshots.folder", serde_json::json!({ "instance": instance })).await
}

/// Opens an instance's `logs/` folder.
#[tauri::command]
async fn open_logs_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    instance: String,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    open_action_path(app, core, "logs.folder", serde_json::json!({ "instance": instance })).await
}

#[tauri::command]
async fn open_backups_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
    instance: String,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    open_action_path(app, core, "worlds.backups_folder", serde_json::json!({ "instance": instance })).await
}

/// Serves screenshots: `shot://localhost/<instance>/<file>` (Windows: `http://shot.localhost/...`).
fn shot_protocol(
    app: &tauri::AppHandle,
    req: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let deny = |code: u16| Response::builder().status(code).body(Vec::new()).unwrap();
    let Some(state) = app.try_state::<AppCore>() else {
        return deny(503);
    };
    let path = req.uri().path().trim_start_matches('/');
    let Some((id, file)) = path.split_once('/') else {
        return deny(404);
    };
    let file = percent_decode(file);
    let Some((bytes, mime)) = fl_core::media::read_shot(&state.0, id, &file) else {
        return deny(404);
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "private, max-age=3600")
        .body(bytes)
        .unwrap()
}

/// `%20` and friends in a URL path → text (invalid escapes stay as they are).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Saves the user's wallpaper. The body is the raw image; its type comes from the bytes.
#[tauri::command]
async fn set_wallpaper(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нужна картинка".into());
    };
    fl_core::wallpaper::save(&state.0.data_dir, bytes).map_err(|e| e.to_string())
}

/// Keeps an uploaded `.mrpack` for `instances.import_mrpack`; returns its token.
#[tauri::command]
async fn stage_mrpack(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, AppCore>,
) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нужен файл .mrpack".into());
    };
    fl_core::mrpack::stage(&state.0.data_dir, bytes).map_err(|e| e.to_string())
}

/// Picks a `.mrpack` with the native dialog and stages it straight from disk, so the
/// WebView never has to read the file (WebView2 throws NotReadableError on some files).
#[tauri::command]
async fn pick_mrpack(app: tauri::AppHandle, state: tauri::State<'_, AppCore>) -> Result<Option<String>, String> {
    let data_dir = state.0.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title(tr("Импорт сборки"))
            .add_filter(tr("Сборка Modrinth"), &["mrpack", "zip"])
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
        else {
            return Ok(None);
        };
        stage_mrpack_file(&data_dir, &path).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn stage_mrpack_file(data_dir: &std::path::Path, path: &std::path::Path) -> Result<String, String> {
    std::fs::metadata(path).map_err(|e| format!("Не удалось открыть файл: {e}"))?;
    fl_core::mrpack::stage_file(data_dir, path).map_err(|e| e.to_string())
}

/// Stages a `.mrpack` dropped onto the window (path from the drag-drop event).
#[tauri::command]
async fn stage_mrpack_path(path: String, state: tauri::State<'_, AppCore>) -> Result<String, String> {
    let data_dir = state.0.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || stage_mrpack_file(&data_dir, std::path::Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves an instance's own icon or cover, already cropped by the UI. Raw body; the
/// instance and kind come in the `x-inst` / `x-kind` headers.
#[tauri::command]
async fn set_art(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, AppCore>,
) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нужна картинка".into());
    };
    let h = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    fl_core::art::save(&state.0.data_dir, &h("x-inst"), &h("x-kind"), bytes).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_art(instance: String, kind: String, state: tauri::State<'_, AppCore>) -> Result<(), String> {
    fl_core::art::clear(&state.0.data_dir, &instance, &kind).map_err(|e| e.to_string())
}

/// Serves own instance pictures: `art://localhost/<id>/<icon|cover>` (Windows: `http://art.localhost/...`).
fn art_protocol(app: &tauri::AppHandle, req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let path = req.uri().path().trim_start_matches('/');
    let found = path.split_once('/').and_then(|(id, kind)| {
        let state = app.try_state::<AppCore>()?;
        if id == "lib" {
            fl_core::art::lib_read(&state.0.data_dir, kind)
        } else {
            fl_core::art::read(&state.0.data_dir, id, kind)
        }
    });
    let Some((bytes, mime)) = found else {
        return Response::builder().status(404).body(Vec::new()).unwrap();
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", if path.starts_with("lib/") { "private, max-age=31536000, immutable" } else { "no-cache" })
        .body(bytes)
        .unwrap()
}

/// Remote images in descriptions: `rimg://localhost/<encoded url>` (Windows:
/// `http://rimg.localhost/...`), fetched and checked by the core.
async fn rimg_protocol(app: &tauri::AppHandle, req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let url = percent_decode(req.uri().path().trim_start_matches('/'));
    let Some(state) = app.try_state::<AppCore>() else {
        return Response::builder().status(503).body(Vec::new()).unwrap();
    };
    let data_dir = state.0.data_dir.clone();
    match fl_core::rimg::get(&data_dir, &url).await {
        Ok((bytes, mime)) => Response::builder()
            .status(200)
            .header("Content-Type", mime)
            .header("X-Content-Type-Options", "nosniff")
            .header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'")
            .header("Cache-Control", "private, max-age=86400")
            .body(bytes)
            .unwrap(),
        Err(_) => Response::builder().status(404).body(Vec::new()).unwrap(),
    }
}

#[tauri::command]
async fn clear_wallpaper(state: tauri::State<'_, AppCore>) -> Result<(), String> {
    fl_core::wallpaper::clear(&state.0.data_dir).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_font(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нужен файл шрифта".into());
    };
    fl_core::fonts::save(&state.0.data_dir, bytes).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_font(state: tauri::State<'_, AppCore>) -> Result<(), String> {
    fl_core::fonts::clear(&state.0.data_dir).map_err(|e| e.to_string())
}

/// Serves the user's font: `font://localhost/f` (Windows: `http://font.localhost/f`).
/// Fonts are fetched in CORS mode, so the page origin has to be allowed.
fn font_protocol(app: &tauri::AppHandle) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let found = app
        .try_state::<AppCore>()
        .and_then(|state| fl_core::fonts::read(&state.0.data_dir));
    let Some((bytes, mime)) = found else {
        return Response::builder().status(404).body(Vec::new()).unwrap();
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
}

/// Serves the user's wallpaper: `wall://localhost/w` (Windows: `http://wall.localhost/w`).
fn wall_protocol(app: &tauri::AppHandle) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let found = app
        .try_state::<AppCore>()
        .and_then(|state| fl_core::wallpaper::read(&state.0.data_dir));
    let Some((bytes, mime)) = found else {
        return Response::builder().status(404).body(Vec::new()).unwrap();
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
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
        .header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'; img-src data:",
        )
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

/// An addon's call. The UI relays it from the addon's sandboxed iframe; the core
/// checks it against exactly the permissions the user approved for that addon.
#[tauri::command]
async fn addon_action(
    state: tauri::State<'_, AppCore>,
    id: String,
    name: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let core = Arc::clone(&state.0);
    let caller = core
        .addon_caller(&id)
        .ok_or("плагин не включён или не одобрен")?;
    core.dispatch(&caller, &name, args.unwrap_or(Value::Null))
        .await
        .map_err(|e| e.to_string())
}

const ADDON_SDK: &str = include_str!("../../ui/addon-sdk.js");

/// `addon://localhost/<id>/<rel>` (Windows: `http://addon.localhost/<id>/<rel>`).
/// Files of enabled, approved addons only. The page may load files of its own
/// addon and nothing else: no network, no other addons, no launcher.
fn addon_protocol(
    app: &tauri::AppHandle,
    req: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use fl_core::addons;
    use tauri::http::Response;
    let deny = |code: u16| Response::builder().status(code).body(Vec::new()).unwrap();
    let Some(state) = app.try_state::<AppCore>() else {
        return deny(503);
    };
    let core = &state.0;
    let path = req.uri().path().trim_start_matches('/');
    let Some((id, rel)) = path.split_once('/') else {
        return deny(404);
    };
    if core.addon_caller(id).is_none() {
        return deny(403);
    }
    let bytes = if rel == "fl-sdk.js" {
        ADDON_SDK.as_bytes().to_vec()
    } else {
        let Some(file) = addons::file(&core.data_dir, id, rel) else {
            return deny(404);
        };
        let Ok(b) = std::fs::read(&file) else {
            return deny(404);
        };
        b
    };
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => return deny(403),
    };
    let own = format!("addon://localhost/{id}/ http://addon.localhost/{id}/");
    let csp = if ext == "html" {
        format!(
            "default-src 'none'; script-src {own}; style-src {own} 'unsafe-inline'; \
             img-src {own} data:; font-src {own}; connect-src 'none'; form-action 'none'; \
             base-uri 'none'; frame-ancestors tauri://localhost http://tauri.localhost"
        )
    } else {
        "default-src 'none'; style-src 'unsafe-inline'; img-src data:".to_owned()
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", csp)
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
}

/// Opens the addons folder in the system file manager.
#[tauri::command]
async fn open_exports_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(&Caller::BuiltinUi, "exports.folder", Value::Null)
        .await
        .map_err(|e| e.to_string())?;
    let path = v["path"].as_str().ok_or("no folder")?.to_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_addons_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    let core = Arc::clone(&state.0);
    let v = core
        .dispatch(&Caller::BuiltinUi, "addons.folder", Value::Null)
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

/// Whether the window's close button hides the launcher to the tray (set by the UI, on by default).
static CLOSE_TO_TRAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[tauri::command]
fn set_close_to_tray(on: bool) {
    CLOSE_TO_TRAY.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The launcher's own icon — window, taskbar and tray — drawn by the UI from Settings → Внешний вид →
/// «Логотип» (glyph colour, plate, shape). An empty `rgba` puts the bundled icon back.
#[tauri::command]
fn set_app_icon(app: tauri::AppHandle, rgba: Vec<u8>, w: u32, h: u32) -> Result<(), String> {
    let img: tauri::image::Image<'static> = if rgba.is_empty() {
        app.default_window_icon()
            .cloned()
            .ok_or_else(|| "no default icon".to_string())?
            .to_owned()
    } else {
        if w == 0 || h == 0 || w > 512 || h > 512 || rgba.len() != (w as usize) * (h as usize) * 4 {
            return Err("bad icon size".into());
        }
        tauri::image::Image::new_owned(rgba, w, h)
    };
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_icon(img.clone());
        #[cfg(windows)]
        if let Ok(hwnd) = win.hwnd() {
            set_big_icon(hwnd.0 as _, &img);
        }
    }
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_icon(Some(img));
    }
    Ok(())
}

/// Tauri sets only the small window icon; the taskbar and Alt+Tab use the big one.
#[cfg(windows)]
fn set_big_icon(hwnd: windows_sys::Win32::Foundation::HWND, img: &tauri::image::Image<'_>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, SendMessageW, ICON_BIG, WM_SETICON};
    static PREV: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut bgra = img.rgba().to_vec();
    let mut and_mask = Vec::with_capacity(bgra.len() / 4);
    for p in bgra.chunks_exact_mut(4) {
        and_mask.push(p[3].wrapping_sub(u8::MAX));
        p.swap(0, 2);
    }
    // SAFETY: both buffers outlive the call and hold w*h pixels; the previous handle is ours to free.
    unsafe {
        let icon = CreateIcon(std::ptr::null_mut(), w, h, 1, 32, and_mask.as_ptr(), bgra.as_ptr());
        if icon.is_null() {
            return;
        }
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
        let old = PREV.swap(icon as usize, std::sync::atomic::Ordering::Relaxed);
        if old != 0 {
            DestroyIcon(old as _);
        }
    }
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

/// Interface strings the native side draws itself (tray menu, window titles, file dialogs),
/// keyed by the Russian source text. Filled by the UI for any language other than Russian.
static UI_STRINGS: std::sync::RwLock<Option<std::collections::HashMap<String, String>>> =
    std::sync::RwLock::new(None);

fn tr(s: &str) -> String {
    UI_STRINGS
        .read()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(s).filter(|v| !v.is_empty()).cloned()))
        .unwrap_or_else(|| s.to_string())
}

const SAFE_TITLE: &str = "Freedom Launcher — безопасные настройки";

#[tauri::command]
fn set_ui_strings(app: tauri::AppHandle, strings: std::collections::HashMap<String, String>) {
    if let Ok(mut m) = UI_STRINGS.write() {
        *m = Some(strings);
    }
    if let (Some(t), Ok(menu)) = (app.tray_by_id("main"), tray_menu(&app)) {
        let _ = t.set_menu(Some(menu));
    }
    if let Some(w) = app.get_webview_window("main") {
        if w.title().is_ok_and(|t| t == SAFE_TITLE) {
            let _ = w.set_title(&tr(SAFE_TITLE));
        }
    }
}

fn tray_menu<R: tauri::Runtime, M: Manager<R>>(app: &M) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let open = MenuItem::with_id(app, "open", tr("Открыть"), true, None::<&str>)?;
    let safe = MenuItem::with_id(app, "safe", tr("Безопасные настройки"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("Выход"), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&open, &safe, &sep, &quit])
}

fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let menu = tray_menu(app)?;
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

/// Writes every panic to `<data>/logs/crash-<unix>.log`. With `panic = "unwind"`
/// a failed background task ends alone and the launcher (and the game) keep running.
fn crash_log_hook(dir: std::path::PathBuf) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let thread = std::thread::current();
        let text = format!(
            "Freedom Launcher {}\nthread: {}\n{}\n\n{}\n",
            fl_core::VERSION,
            thread.name().unwrap_or("?"),
            info,
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("crash-{now}.log")), text);
        prev(info);
    }));
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::UpdateState::default())
        .register_uri_scheme_protocol("theme", |ctx, req| theme_protocol(ctx.app_handle(), &req))
        .register_uri_scheme_protocol("addon", |ctx, req| addon_protocol(ctx.app_handle(), &req))
        .register_uri_scheme_protocol("wall", |ctx, _req| wall_protocol(ctx.app_handle()))
        .register_uri_scheme_protocol("font", |ctx, _req| font_protocol(ctx.app_handle()))
        .register_uri_scheme_protocol("shot", |ctx, req| shot_protocol(ctx.app_handle(), &req))
        .register_uri_scheme_protocol("art", |ctx, req| art_protocol(ctx.app_handle(), &req))
        .register_asynchronous_uri_scheme_protocol("rimg", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move { responder.respond(rimg_protocol(&app, &req).await) });
        })
        .setup(move |app| {
            tray(app)?;
            // Safe settings always quit on close; the normal window goes to the tray.
            if let (false, Some(w)) = (safe_mode, app.get_webview_window("main")) {
                let win = w.clone();
                w.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        if CLOSE_TO_TRAY.load(std::sync::atomic::Ordering::Relaxed) {
                            api.prevent_close();
                            let _ = win.hide();
                        }
                    }
                });
            }
            let data_dir = app.path().app_data_dir()?;
            crash_log_hook(data_dir.join("logs"));
            let core = Core::new(data_dir, started, safe_mode)?;
            if !safe_mode {
                if let Err(e) = fl_core::bundled::install(&core.data_dir) {
                    eprintln!("bundled addons: {e}");
                }
            }
            app.manage(AppCore(core));
            if safe_mode {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_title(SAFE_TITLE);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            action,
            set_close_to_tray,
            set_app_icon,
            set_ui_strings,
            theme_action,
            open_login_page,
            ms_window_login,
            open_link,
            open_instance_folder,
            open_world_folder,
            open_shots_folder,
            open_logs_folder,
            open_backups_folder,
            open_themes_folder,
            addon_action,
            open_addons_folder,
            pick_addon,
            open_exports_folder,
            set_wallpaper,
            clear_wallpaper,
            set_font,
            clear_font,
            set_art,
            clear_art,
            stage_mrpack,
            pick_mrpack,
            stage_mrpack_path,
            pick_folder,
            reveal_content,
            pick_content,
            drop_content,
            updater::update_check,
            updater::update_install,
            updater::update_progress
        ])
        .build(tauri::generate_context!())
        .expect("failed to start Freedom Launcher")
        .run(|_, _| {});
}

#[cfg(test)]
mod link_tests {
    #[test]
    fn percent_decoding() {
        assert_eq!(super::percent_decode("2026-10-16_21.04.46.png"), "2026-10-16_21.04.46.png");
        assert_eq!(super::percent_decode("a%20b.png"), "a b.png");
        assert_eq!(super::percent_decode("%D0%BC.png"), "м.png");
        assert_eq!(super::percent_decode("bad%zz%"), "bad%zz%");
        assert_eq!(super::percent_decode("м%"), "м%");
    }
    use super::link_ok;

    #[test]
    fn links_are_plain_https() {
        assert!(link_ok("https://github.com/CaffeineMC/sodium"));
        assert!(link_ok("https://discord.gg/abc?x=1"));
        assert!(!link_ok("http://example.com"));
        assert!(!link_ok("file:///etc/passwd"));
        assert!(!link_ok("https://user@evil.com/"));
        assert!(!link_ok("https:///x"));
        assert!(!link_ok("https://a.com/ b"));
    }
}
