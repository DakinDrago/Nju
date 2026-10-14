//! Moteur local : exécute un modèle wav2vec2/MMS exporté en ONNX, sans Python ni serveur.
//! Les modèles sont des sous-dossiers de `<données de l'appli>/models/` contenant
//! model.onnx (+ model.onnx.data), vocab.json et meta.json (produits par export_onnx.py).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ort::{session::Session, value::Tensor};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::ctc::{self, Meta};
use crate::fsops;

const MAX_CHUNK_SECS: f32 = 25.0;
const MIN_CHUNK_SAMPLES: usize = 1600; // 0,1 s à 16 kHz

struct Loaded {
    name: String,
    session: Session,
    meta: Meta,
    /// Périphérique réellement utilisé, affiché dans l'interface (ex. « CPU », « GPU (CUDA) »).
    device: String,
}

/// Un seul modèle chargé à la fois (le précédent est libéré avant d'en charger un autre).
pub struct LocalState {
    slot: Arc<Mutex<Option<Loaded>>>,
    /// Mode de périphérique : "auto" (meilleur dispo selon l'OS), "cuda" (Windows/Linux,
    /// opt-in `--features gpu`), "directml" (Windows), "webgpu" (Linux),
    /// "coreml" (macOS), "cpu" (universel).
    device_mode: Arc<Mutex<String>>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
            device_mode: Arc::new(Mutex::new("auto".to_string())),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelInfo {
    id: String,
    name: String,
    size_mb: u64,
}

fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    fs::create_dir_all(&dir).map_err(|e| format!("Impossible de créer {} : {e}", dir.display()))?;
    Ok(dir)
}

fn model_dir(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    if !fsops::valid_name(name) {
        return Err("Nom de modèle invalide".into());
    }
    Ok(models_root(app)?.join(name))
}

fn dir_size_mb(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum::<u64>()
                / 1_000_000
        })
        .unwrap_or(0)
}

/// Nom de la lib ONNX Runtime partagée par OS.
fn ort_lib_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// Localise la lib ONNX Runtime SANS appeler ort : si elle est introuvable,
/// on renvoie une erreur propre au lieu du panic d'ort (`expect`, fatal avec
/// `panic = "abort"` en release → exit 134). Miroir de la résolution d'ort :
/// 1. `ORT_DYLIB_PATH` (posée par setup() pour les paquets, ou par l'utilisateur),
/// 2. à côté de l'exécutable (dev / binaire direct, géré par build.rs).
pub(crate) fn ort_library_path() -> Result<PathBuf, String> {
    if let Ok(s) = std::env::var("ORT_DYLIB_PATH") {
        if !s.is_empty() {
            let p = PathBuf::from(&s);
            return p.exists().then_some(p).ok_or_else(|| {
                format!(
                    "ONNX Runtime introuvable : ORT_DYLIB_PATH pointe vers un fichier absent ({s})"
                )
            });
        }
    }
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("Impossible de localiser l'exécutable : {e}"))?
        .parent()
        .ok_or("Chemin d'exécutable invalide")?
        .to_path_buf();
    let p = exe_dir.join(ort_lib_filename());
    if p.exists() {
        return Ok(p);
    }
    Err("ONNX Runtime introuvable à côté de l'exécutable : réinstallez l'application (moteur local indisponible, le mode serveur reste utilisable)".into())
}

/// Crée la session ONNX. Ordre de préférence selon le mode et l'OS :
/// - "auto" : Windows (CUDA → DirectML → CPU), Linux (CUDA → WebGPU → CPU), macOS (CoreML → CPU)
/// - "cuda" : CUDA seulement (Windows/Linux, opt-in explicite `--features gpu`, sinon erreur)
/// - "directml" / "webgpu" / "coreml" : EP natif de l'OS, erreur sinon
/// - "cpu" : CPU uniquement (universel)
/// Notes : pas de CUDA sur macOS (même en build gpu) ; en build gpu Linux,
/// la lib -gpu MS n'a pas l'EP WebGPU (ordre réel : CUDA → CPU).
fn make_session(path: &Path, mode: &str) -> Result<(Session, String), String> {
    // Vérifie la lib AVANT tout appel ort (voir ort_library_path).
    ort_library_path()?;

    // 1. Essaie CUDA si mode "auto" ou "cuda" (Windows/Linux + opt-in `--features gpu`).
    if mode == "auto" || mode == "cuda" {
        #[cfg(all(feature = "gpu", any(target_os = "windows", target_os = "linux")))]
        {
            let attempt = Session::builder()
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::CUDA::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU (CUDA)".to_string())),
                Err(e) => {
                    let why: String = e.to_string().chars().take(120).collect();
                    if mode == "cuda" {
                        return Err(format!("CUDA requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers l'EP natif de l'OS
                    eprintln!("CUDA indisponible, tentative suivante : {why}");
                }
            }
        }
        // Pas de CUDA sur macOS, même en build gpu (CoreML est l'EP natif).
        #[cfg(target_os = "macos")]
        if mode == "cuda" {
            return Err("Pas de CUDA sur Mac (CoreML est l'EP natif, mode coreml/auto)".into());
        }
        #[cfg(all(not(feature = "gpu"), not(target_os = "macos")))]
        if mode == "cuda" {
            return Err("CUDA non compilé (build dédié `--features gpu` requis)".into());
        }
    }

    // 2. Essaie DirectML si mode "auto" ou "directml" (Windows, natif par défaut).
    if mode == "auto" || mode == "directml" {
        #[cfg(target_os = "windows")]
        {
            let attempt = Session::builder()
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::DirectML::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU (DirectML)".to_string())),
                Err(e) => {
                    let why: String = e.to_string().chars().take(120).collect();
                    if mode == "directml" {
                        return Err(format!("DirectML requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("DirectML indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        if mode == "directml" {
            return Err("DirectML n'existe que sur Windows".into());
        }
    }

    // 2b. Essaie WebGPU si mode "auto" ou "webgpu" (Linux, natif par défaut).
    if mode == "auto" || mode == "webgpu" {
        #[cfg(target_os = "linux")]
        {
            let attempt = Session::builder()
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::WebGPU::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU (WebGPU)".to_string())),
                Err(e) => {
                    let why: String = e.to_string().chars().take(120).collect();
                    if mode == "webgpu" {
                        return Err(format!("WebGPU requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("WebGPU indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        if mode == "webgpu" {
            return Err("WebGPU n'est proposé que sur Linux".into());
        }
    }

    // 2c. Essaie CoreML si mode "auto" ou "coreml" (macOS, natif par défaut).
    if mode == "auto" || mode == "coreml" {
        #[cfg(target_os = "macos")]
        {
            let attempt = Session::builder()
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::CoreML::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU (CoreML)".to_string())),
                Err(e) => {
                    let why: String = e.to_string().chars().take(120).collect();
                    if mode == "coreml" {
                        return Err(format!("CoreML requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("CoreML indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        if mode == "coreml" {
            return Err("CoreML n'existe que sur macOS".into());
        }
    }

    // 3. CPU fallback
    let mut builder = Session::builder().map_err(|e| e.to_string())?;
    let session = builder
        .commit_from_file(path)
        .map_err(|e| format!("Chargement ONNX impossible : {e}"))?;
    Ok((session, "CPU".to_string()))
}

fn load_blocking(dir: &Path, name: &str, mode: &str) -> Result<Loaded, String> {
    for f in ["model.onnx", "vocab.json", "meta.json"] {
        if !dir.join(f).exists() {
            return Err(format!("Fichier manquant dans le dossier du modèle : {f}"));
        }
    }
    let meta_json = fs::read_to_string(dir.join("meta.json")).map_err(|e| e.to_string())?;
    let vocab_json = fs::read_to_string(dir.join("vocab.json")).map_err(|e| e.to_string())?;
    let meta = ctc::load_meta(&meta_json, &vocab_json)?;

    let (session, device) = make_session(&dir.join("model.onnx"), mode)?;

    Ok(Loaded {
        name: name.to_string(),
        session,
        meta,
        device,
    })
}

fn ensure_loaded(
    slot: &mut Option<Loaded>,
    dir: &Path,
    name: &str,
    mode: &str,
) -> Result<(), String> {
    if slot.as_ref().is_some_and(|l| l.name == name) {
        return Ok(());
    }
    *slot = None; // libère la mémoire de l'ancien modèle d'abord
    *slot = Some(load_blocking(dir, name, mode)?);
    Ok(())
}

fn run_chunk(loaded: &mut Loaded, mut audio: Vec<f32>) -> Result<String, String> {
    if loaded.meta.do_normalize {
        ctc::normalize(&mut audio);
    }
    let n = audio.len();
    let input = Tensor::from_array(([1_usize, n], audio)).map_err(|e| e.to_string())?;
    let outputs = loaded
        .session
        .run(ort::inputs![input])
        .map_err(|e| format!("Inférence ONNX : {e}"))?;
    let (_shape, logits) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    Ok(ctc::decode_greedy(logits, &loaded.meta))
}

fn transcribe_blocking(loaded: &mut Loaded, wav: &[u8], chunk_secs: f32) -> Result<String, String> {
    let (samples, rate) = ctc::parse_wav(wav)?;
    let audio = ctc::resample(&samples, rate, loaded.meta.sample_rate);
    if audio.len() < MIN_CHUNK_SAMPLES {
        return Err("Audio trop court".into());
    }

    let mut parts = Vec::new();
    for (start, end) in ctc::split_chunks(&audio, loaded.meta.sample_rate, chunk_secs) {
        if end - start < MIN_CHUNK_SAMPLES {
            continue;
        }
        let text = run_chunk(loaded, audio[start..end].to_vec())?;
        if !text.is_empty() {
            parts.push(text);
        }
    }
    Ok(parts.join(" "))
}

#[tauri::command]
pub fn models_dir(app: AppHandle) -> Result<String, String> {
    Ok(models_root(&app)?.display().to_string())
}

#[tauri::command]
pub fn list_local_models(app: AppHandle) -> Result<Vec<LocalModelInfo>, String> {
    let root = models_root(&app)?;
    let mut models: Vec<LocalModelInfo> = fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_dir()) // suit les liens ; ignore un lien dont la cible est absente
        .filter(|e| fsops::valid_name(&e.file_name().to_string_lossy()))
        .filter(|e| fsops::has_model_files(&e.path()))
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            LocalModelInfo {
                id: name.clone(),
                size_mb: dir_size_mb(&e.path()),
                name,
            }
        })
        .collect();
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

#[tauri::command]
pub async fn load_local_model(
    app: AppHandle,
    state: State<'_, LocalState>,
    model: String,
) -> Result<String, String> {
    let dir = model_dir(&app, &model)?;
    let slot = state.slot.clone();
    let mode = state
        .device_mode
        .lock()
        .map_err(|_| "Verrou device_mode indisponible".to_string())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, &mode)?;
        let device = guard.as_ref().map(|l| l.device.clone()).unwrap_or_default();
        Ok(format!("Prêt · {device}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn transcribe_local(
    app: AppHandle,
    state: State<'_, LocalState>,
    model: String,
    audio_b64: String,
    chunk_secs: Option<f32>,
) -> Result<String, String> {
    let dir = model_dir(&app, &model)?;
    let wav = STANDARD.decode(audio_b64).map_err(|e| e.to_string())?;
    let slot = state.slot.clone();
    let mode = state
        .device_mode
        .lock()
        .map_err(|_| "Verrou device_mode indisponible".to_string())?
        .clone();
    // Durée max par morceau (réglable dans Paramètres), bornée pour rester raisonnable.
    let chunk_secs = chunk_secs.unwrap_or(MAX_CHUNK_SECS).clamp(5.0, 120.0);
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, &mode)?;
        let loaded = guard.as_mut().ok_or("Aucun modèle chargé")?;
        transcribe_blocking(loaded, &wav, chunk_secs)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Importe un dossier exporté (model.onnx, vocab.json, meta.json) dans le dossier des modèles.
/// mode = "copy" (copie le modèle dans l'appli) ou "link" (lien : le dossier d'origine doit rester en place).
#[tauri::command]
pub async fn import_local_model(
    app: AppHandle,
    source: String,
    mode: String,
) -> Result<String, String> {
    let src = PathBuf::from(&source);
    if !src.is_dir() || !fsops::has_model_files(&src) {
        return Err(
            "Ce dossier n'est pas un modèle exporté (model.onnx, vocab.json et meta.json attendus)"
                .into(),
        );
    }
    let name = src
        .file_name()
        .ok_or("Nom de dossier invalide")?
        .to_string_lossy()
        .to_string();
    let dst = model_dir(&app, &name)?;
    if fs::symlink_metadata(&dst).is_ok() {
        return Err(format!(
            "Un modèle nommé « {name} » existe déjà : retire-le d'abord"
        ));
    }
    let root = models_root(&app)?;

    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        if mode == "link" {
            fsops::link_dir(&src, &dst)?;
        } else {
            fsops::copy_model(&src, &root, &name, &dst)?;
        }
        Ok(name)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Retire un modèle de l'appli. Pour un lien, seul le lien est supprimé.
#[tauri::command]
pub async fn remove_local_model(
    app: AppHandle,
    state: State<'_, LocalState>,
    model: String,
) -> Result<(), String> {
    let dir = model_dir(&app, &model)?;
    let slot = state.slot.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        if guard.as_ref().is_some_and(|l| l.name == model) {
            *guard = None; // libère le fichier avant de le supprimer
        }
        fsops::remove_model(&dir)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Choisit le périphérique : "auto" (meilleur dispo selon l'OS), "cuda" (Windows/Linux,
/// opt-in `--features gpu` uniquement), "directml" (Windows), "webgpu" (Linux),
/// "coreml" (macOS), "cpu" (universel). Le modèle chargé est libéré pour que
/// le prochain chargement applique le nouveau choix.
#[tauri::command]
pub async fn set_local_device(state: State<'_, LocalState>, device: String) -> Result<(), String> {
    // Valide le mode
    let valid_modes = ["auto", "cuda", "directml", "webgpu", "coreml", "cpu"];
    if !valid_modes.contains(&device.as_str()) {
        return Err(format!(
            "Mode invalide : {device}. Modes valides : {}",
            valid_modes.join(", ")
        ));
    }

    *state
        .device_mode
        .lock()
        .map_err(|_| "Verrou device_mode indisponible".to_string())? = device;
    let slot = state.slot.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        *guard = None;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
