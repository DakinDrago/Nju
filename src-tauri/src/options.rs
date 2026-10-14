//! Options applicatives modifiables depuis les Paramètres :
//! raccourci global (hotkey) et comportement à la fermeture (rester en fond ou quitter).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use ort::ep::ExecutionProvider;

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
pub async fn set_run_in_background(
    state: State<'_, AppOptions>,
    enabled: bool,
) -> Result<(), String> {
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
///
/// Chaque flag est une SONDE RUNTIME (et non un `cfg!` de compilation) :
/// le flag Cargo seul ne suffit pas, car la lib ONNX Runtime chargée
/// dynamiquement peut ne pas contenir l'EP — ex. lib officielle Microsoft
/// sans EP WebGPU (erreur « not supported in this build ») alors que le
/// code Rust est compilé avec le feature `webgpu`. Sans lib ORT trouvable,
/// tout est à `false` (le moteur local est de toute façon indisponible).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub cuda: bool,
    pub directml: bool,
    pub webgpu: bool,
    pub coreml: bool,
    pub platform: String,
}

/// Teste si un EP est compilé dans la lib ORT chargée (sans modèle).
/// `is_available()` interroge les providers enregistrés dans le binaire
/// ORT : `false` = « not supported in this build » garanti au chargement.
fn ep_available(probe: impl FnOnce() -> bool) -> bool {
    // Garde-fou : sans lib, ort paniquerait à l'init de l'environnement.
    if crate::local::ort_library_path().is_err() {
        return false;
    }
    probe()
}

#[tauri::command]
pub async fn get_capabilities() -> Result<Capabilities, String> {
    // CUDA : opt-in `--features gpu` + présent dans la lib.
    #[cfg(feature = "gpu")]
    let cuda = ep_available(|| ort::ep::CUDA::default().is_available().unwrap_or(false));
    #[cfg(not(feature = "gpu"))]
    let cuda = false;
    // EPs natifs par OS : le `cfg` garde la compilation par target, la sonde
    // vérifie la lib réellement chargée.
    #[cfg(target_os = "windows")]
    let directml = ep_available(|| ort::ep::DirectML::default().is_available().unwrap_or(false));
    #[cfg(not(target_os = "windows"))]
    let directml = false;
    #[cfg(target_os = "linux")]
    let webgpu = ep_available(|| ort::ep::WebGPU::default().is_available().unwrap_or(false));
    #[cfg(not(target_os = "linux"))]
    let webgpu = false;
    #[cfg(target_os = "macos")]
    let coreml = ep_available(|| ort::ep::CoreML::default().is_available().unwrap_or(false));
    #[cfg(not(target_os = "macos"))]
    let coreml = false;
    Ok(Capabilities {
        cuda,
        directml,
        webgpu,
        coreml,
        platform: std::env::consts::OS.to_string(),
    })
}
