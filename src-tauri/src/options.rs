//! Options applicatives modifiables depuis les Paramètres :
//! raccourci global (hotkey) et comportement à la fermeture (rester en fond ou quitter).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[cfg(any(target_os = "linux", target_os = "macos"))]
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

/// Ce que ce binaire peut réellement utiliser : l'interface propose
/// GPU quand l'accélération native de l'OS est disponible, sinon CPU.
/// Chaque OS embarque son EP natif.
///
/// Le flag est une SONDE RUNTIME (et non un `cfg!` de compilation) :
/// le feature Cargo seul ne suffit pas, car la lib ONNX Runtime chargée
/// dynamiquement peut ne pas contenir l'EP — ex. lib officielle Microsoft
/// sans EP natif (erreur « not supported in this build ») alors que le
/// code Rust est compilé avec le feature. Sans lib ORT trouvable,
/// tout est à `false` (le moteur local est de toute façon indisponible).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub gpu: bool,
    /// Raison lisible quand `gpu == false` (affichée par l'interface).
    pub gpu_reason: Option<String>,
    pub platform: String,
}

/// Windows : enregistrer l'EP DirectML est une sonde complète.
///
/// `is_available()` ne vérifie que la présence de l'API DML dans la lib ORT
/// chargée : il renvoie `true` même si `DirectML.dll` est absente/trop
/// ancienne ou si aucun GPU DirectX 12 n'est utilisable. L'enregistrement,
/// lui, force ORT à créer l'adaptateur DXCore, le device D3D12 puis le
/// device DirectML (`DMLCreateDevice1`, feature level 5.0) : toutes les
/// causes d'échec réelles sont donc couvertes (DXCore/D3D12/DirectML).
#[cfg(target_os = "windows")]
fn probe_directml() -> Result<(), String> {
    let builder =
        ort::session::Session::builder().map_err(|e| format!("ONNX Runtime indisponible : {e}"))?;
    builder
        .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
        .map_err(|e| directml_reason(&e.to_string()))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn directml_reason(detail: &str) -> String {
    let m = detail.to_lowercase();
    let hint = if m.contains("specified module")
        || m.contains("specified procedure")
        || m.contains("0x8007007e")
        || m.contains("0x8007007f")
    {
        "DirectML.dll est introuvable ou incompatible : réinstalle l'application."
    } else if m.contains("no gpu")
        || m.contains("no devices detected")
        || m.contains("dxcore")
        || m.contains("887a0004")
    {
        "Aucun GPU compatible DirectX 12 utilisable n'a été détecté."
    } else if m.contains("feature") || m.contains("e_nointerface") || m.contains("0x80004002") {
        "Le pilote graphique ne prend pas en charge DirectML 5.0 (DirectX 12)."
    } else {
        "DirectML n'a pas pu démarrer sur cet ordinateur."
    };
    let why: String = detail.chars().take(160).collect();
    format!("{hint} Détail : {why}")
}

#[tauri::command]
pub async fn get_capabilities() -> Result<Capabilities, String> {
    let mut caps = Capabilities {
        gpu: false,
        gpu_reason: None,
        platform: std::env::consts::OS.to_string(),
    };

    // Garde-fou : sans lib ORT, toute sonde ferait paniquer l'init d'ort.
    if let Err(e) = crate::local::ort_library_path() {
        caps.gpu_reason = Some(e);
        return Ok(caps);
    }

    // EP natif par OS : le `cfg` garde la compilation par target, la sonde
    // vérifie la lib réellement chargée (et, pour DirectML, le GPU).
    #[cfg(target_os = "windows")]
    match probe_directml() {
        Ok(()) => caps.gpu = true,
        Err(reason) => caps.gpu_reason = Some(reason),
    }
    #[cfg(target_os = "linux")]
    if ort::ep::WebGPU::default().is_available().unwrap_or(false) {
        caps.gpu = true;
    } else {
        caps.gpu_reason = Some(
            "WebGPU n'est pas disponible (build ONNX Runtime ou pilote Vulkan incompatible)."
                .to_string(),
        );
    }
    #[cfg(target_os = "macos")]
    if ort::ep::CoreML::default().is_available().unwrap_or(false) {
        caps.gpu = true;
    } else {
        caps.gpu_reason = Some("CoreML n'est pas disponible sur cet ordinateur.".to_string());
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        caps.gpu_reason = Some("Accélération GPU non prise en charge sur cet OS.".to_string());
    }

    Ok(caps)
}
