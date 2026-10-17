//! In-app updates (Settings → Лаунчер → «Обновления»): the launcher checks the
//! release feed, downloads the new installer, verifies its minisign signature
//! against the key built into the app, installs it over itself and restarts.
//! Instances, settings and accounts live in the data folder and are kept.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

use serde_json::{json, Value};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    done: AtomicU64,
    total: AtomicU64,
}

/// `{ available, version, current, notes, date }`; keeps the found update for `update_install`.
#[tauri::command]
pub async fn update_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<Value, String> {
    let current = app.package_info().version.to_string();
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Не удалось проверить обновления: {e}"))?;
    let out = match &found {
        Some(u) => json!({
            "available": true,
            "version": u.version,
            "current": current,
            "notes": u.body,
            "date": u.date.map(|d| d.to_string()),
        }),
        None => json!({ "available": false, "current": current }),
    };
    *state.pending.lock().map_err(|e| e.to_string())? = found;
    Ok(out)
}

/// Downloads and installs the update found by `update_check`, then restarts.
#[tauri::command]
pub async fn update_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<(), String> {
    let update = state
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("Сначала проверьте обновления")?;
    state.done.store(0, Ordering::Relaxed);
    state.total.store(0, Ordering::Relaxed);
    let (done, total) = (&state.done, &state.total);
    update
        .download_and_install(
            |chunk, len| {
                done.fetch_add(chunk as u64, Ordering::Relaxed);
                if let Some(l) = len {
                    total.store(l, Ordering::Relaxed);
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("Не удалось установить обновление: {e}"))?;
    app.restart();
}

/// `{ done, total }` bytes of the running download.
#[tauri::command]
pub fn update_progress(state: tauri::State<'_, UpdateState>) -> Value {
    json!({
        "done": state.done.load(Ordering::Relaxed),
        "total": state.total.load(Ordering::Relaxed),
    })
}