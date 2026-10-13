//! Options applicatives modifiables depuis les Paramètres :
//! raccourci global (hotkey) et comportement à la fermeture (rester en fond ou quitter).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub struct AppOptions {
    pub run_in_background: Mutex<bool>,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            run_in_background: Mutex::new(true),
        }
    }
}

/// (Ré)enregistre le raccourci global. `enabled = false` = aucun raccourci actif.
/// Le format attendu est celui de global-shortcut, ex. "Ctrl+Shift+Space".
#[tauri::command]
pub async fn set_hotkey(app: AppHandle, enabled: bool, shortcut: String) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    if !enabled {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(shortcut.as_str(), |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                // Affiche la fenêtre pour le retour visuel, puis bascule l'enregistrement
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit("toggle-recording", ());
            }
        })
        .map_err(|e| format!("Raccourci « {shortcut} » refusé : {e}"))?;
    Ok(())
}

/// Choisit le comportement à la fermeture de la fenêtre :
/// `true` = masquer en fond (tray), `false` = quitter l'application.
#[tauri::command]
pub async fn set_run_in_background(state: State<'_, AppOptions>, enabled: bool) -> Result<(), String> {
    *state
        .run_in_background
        .lock()
        .map_err(|_| "Verrou des options indisponible".to_string())? = enabled;
    Ok(())
}

/// Ce que ce binaire peut réellement utiliser : l'interface n'affiche
/// que les modes GPU possibles. Seul CUDA est opt-in explicite
/// (`--features gpu`) ; chaque OS embarque son EP natif par défaut
/// (DirectML/Windows, WebGPU/Linux, CoreML/macOS).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub cuda: bool,
    pub directml: bool,
    pub webgpu: bool,
    pub coreml: bool,
    pub platform: String,
}

#[tauri::command]
pub async fn get_capabilities() -> Result<Capabilities, String> {
    Ok(Capabilities {
        cuda: cfg!(feature = "gpu"),
        directml: cfg!(target_os = "windows"),
        webgpu: cfg!(target_os = "linux"),
        coreml: cfg!(target_os = "macos"),
        platform: std::env::consts::OS.to_string(),
    })
}
