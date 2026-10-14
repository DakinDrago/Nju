//! Moteur local : exécute un modèle wav2vec2/MMS exporté en ONNX, sans Python ni serveur.
//! Les modèles sont des sous-dossiers de `<données de l'appli>/models/` contenant
//! model.onnx (+ model.onnx.data), vocab.json et meta.json (produits par export_onnx.py).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ort::{
    session::{
        Session,
        builder::{BuilderResult, SessionBuilder},
    },
    value::Tensor,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ctc::{self, Meta};
use crate::fsops;

const MAX_CHUNK_SECS: f32 = 25.0;
const MIN_CHUNK_SAMPLES: usize = 1600; // 0,1 s à 16 kHz

struct Loaded {
    name: String,
    session: Session,
    meta: Meta,
    /// Périphérique réellement utilisé, affiché dans l'interface (« CPU », « GPU »).
    device: String,
}

/// Un seul modèle chargé à la fois (le précédent est libéré avant d'en charger un autre).
pub struct LocalState {
    slot: Arc<Mutex<Option<Loaded>>>,
    /// Mode de périphérique : "auto" (GPU si disponible, sinon CPU),
    /// "gpu" (GPU requis, erreur sinon), "cpu" (universel).
    device_mode: Arc<Mutex<String>>,
    /// Threads CPU imposés aux sessions ONNX (None = défaut ORT).
    cpu_threads: Arc<Mutex<Option<usize>>>,
    /// Génération de la transcription en cours : `cancel_transcription`
    /// l'incrémente pour interrompre le découpage après le chunk courant.
    run_id: Arc<Mutex<u64>>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
            device_mode: Arc::new(Mutex::new("auto".to_string())),
            cpu_threads: Arc::new(Mutex::new(None)),
            run_id: Arc::new(Mutex::new(0)),
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

/// Détecte une erreur d'allocation mémoire (RAM/VRAM pleine) dans un message ORT.
fn is_oom_message(msg: &str) -> bool {
    let m = msg.to_lowercase();
    [
        "out of memory",
        "out-of-memory",
        "insufficient memory",
        "not enough memory",
        "failed to allocate",
        "allocation failed",
        "bad_alloc",
        "bad alloc",
        "memory budget",
        "exceeds available",
        "cuda error 2",
    ]
    .iter()
    .any(|k| m.contains(k))
}

/// Vrai si une erreur déjà mise en forme signale un manque de mémoire (déclenche le retry).
fn is_oom_error(err: &str) -> bool {
    err.starts_with("Mémoire insuffisante")
}

/// Erreur de chargement enrichie quand la cause est la mémoire.
fn oom_load_error(e: impl ToString) -> String {
    let full = e.to_string();
    if is_oom_message(&full) {
        let why: String = full.chars().take(120).collect();
        format!(
            "Mémoire insuffisante pour charger ce modèle (RAM/VRAM pleine). Ferme d'autres applis ou passe en CPU avec un morceau plus court. Détail : {why}"
        )
    } else {
        format!("Chargement ONNX impossible : {full}")
    }
}

/// Erreur d'inférence enrichie quand la cause est la mémoire.
fn oom_infer_error(e: impl ToString) -> String {
    let full = e.to_string();
    if is_oom_message(&full) {
        let why: String = full.chars().take(120).collect();
        format!(
            "Mémoire insuffisante pendant la transcription. Réessaie avec un réglage Morceau plus court, ou en CPU. Détail : {why}"
        )
    } else {
        format!("Inférence ONNX : {full}")
    }
}

/// Applique le nombre de threads CPU imposé (None = défaut ORT).
fn apply_threads(b: SessionBuilder, threads: Option<usize>) -> BuilderResult {
    match threads {
        Some(n) => b.with_intra_threads(n),
        None => Ok(b),
    }
}

/// Crée la session ONNX. Ordre de préférence selon le mode :
/// - "auto" : GPU natif de l'OS si disponible, sinon CPU
/// - "gpu" : GPU natif seulement, erreur sinon
/// - "cpu" : CPU uniquement (universel)
fn make_session(path: &Path, mode: &str, threads: Option<usize>) -> Result<(Session, String), String> {
    // Vérifie la lib AVANT tout appel ort (voir ort_library_path).
    ort_library_path()?;

    // Essaie le GPU natif si mode "auto" ou "gpu".
    if mode == "auto" || mode == "gpu" {
        #[cfg(target_os = "windows")]
        {
            let attempt = Session::builder()
                .and_then(|b| Ok(apply_threads(b, threads)?))
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::DirectML::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU".to_string())),
                Err(e) => {
                    let full = e.to_string();
                    if mode == "gpu" && is_oom_message(&full) {
                        return Err(oom_load_error(&full));
                    }
                    let why: String = full.chars().take(120).collect();
                    if mode == "gpu" {
                        return Err(format!("GPU requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("GPU indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let attempt = Session::builder()
                .and_then(|b| Ok(apply_threads(b, threads)?))
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::WebGPU::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU".to_string())),
                Err(e) => {
                    let full = e.to_string();
                    if mode == "gpu" && is_oom_message(&full) {
                        return Err(oom_load_error(&full));
                    }
                    let why: String = full.chars().take(120).collect();
                    if mode == "gpu" {
                        return Err(format!("GPU requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("GPU indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            let attempt = Session::builder()
                .and_then(|b| Ok(apply_threads(b, threads)?))
                .and_then(|b| {
                    Ok(b.with_execution_providers([ort::ep::CoreML::default()
                        .build()
                        .error_on_failure()])?)
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU".to_string())),
                Err(e) => {
                    let full = e.to_string();
                    if mode == "gpu" && is_oom_message(&full) {
                        return Err(oom_load_error(&full));
                    }
                    let why: String = full.chars().take(120).collect();
                    if mode == "gpu" {
                        return Err(format!("GPU requis mais indisponible : {why}"));
                    }
                    // mode == "auto" : on continue vers CPU
                    eprintln!("GPU indisponible, fallback CPU : {why}");
                }
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        if mode == "gpu" {
            return Err("GPU non pris en charge sur cet OS".into());
        }
    }

    // CPU (repli, ou choix explicite).
    let builder = Session::builder().map_err(|e| e.to_string())?;
    let mut builder = apply_threads(builder, threads).map_err(|e| e.to_string())?;
    let session = builder.commit_from_file(path).map_err(oom_load_error)?;
    Ok((session, "CPU".to_string()))
}

fn load_blocking(
    dir: &Path,
    name: &str,
    mode: &str,
    threads: Option<usize>,
) -> Result<Loaded, String> {
    for f in ["model.onnx", "vocab.json", "meta.json"] {
        if !dir.join(f).exists() {
            return Err(format!("Fichier manquant dans le dossier du modèle : {f}"));
        }
    }
    let meta_json = fs::read_to_string(dir.join("meta.json")).map_err(|e| e.to_string())?;
    let vocab_json = fs::read_to_string(dir.join("vocab.json")).map_err(|e| e.to_string())?;
    let meta = ctc::load_meta(&meta_json, &vocab_json)?;

    let (session, device) = make_session(&dir.join("model.onnx"), mode, threads)?;

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
    threads: Option<usize>,
) -> Result<(), String> {
    if slot.as_ref().is_some_and(|l| l.name == name) {
        return Ok(());
    }
    *slot = None; // libère la mémoire de l'ancien modèle d'abord
    *slot = Some(load_blocking(dir, name, mode, threads)?);
    Ok(())
}

fn run_chunk(loaded: &mut Loaded, audio: &[f32]) -> Result<String, String> {
    // Une seule copie : le Tensor exige un Vec possédé.
    let mut buf = audio.to_vec();
    if loaded.meta.do_normalize {
        ctc::normalize(&mut buf);
    }
    let n = buf.len();
    let input = Tensor::from_array(([1_usize, n], buf)).map_err(oom_infer_error)?;
    let outputs = loaded
        .session
        .run(ort::inputs![input])
        .map_err(oom_infer_error)?;
    let (_shape, logits) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    Ok(ctc::decode_greedy(logits, &loaded.meta))
}

/// Progression émise vers l'interface après chaque morceau transcrit.
#[derive(Serialize, Clone)]
struct TranscribeProgress {
    done: usize,
    total: usize,
}

fn transcribe_blocking(
    loaded: &mut Loaded,
    wav: &[u8],
    chunk_secs: f32,
    app: &AppHandle,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<String, String> {
    let (samples, rate) = ctc::parse_wav(wav)?;
    let audio = ctc::resample(&samples, rate, loaded.meta.sample_rate);
    if audio.len() < MIN_CHUNK_SAMPLES {
        return Err("Audio trop court".into());
    }

    let chunks = ctc::split_chunks(&audio, loaded.meta.sample_rate, chunk_secs);
    let total = chunks.len();
    let emit = |done: usize| {
        let _ = app.emit(
            "transcribe-progress",
            TranscribeProgress { done, total },
        );
    };
    emit(0);

    let mut parts = Vec::new();
    for (i, (start, end)) in chunks.into_iter().enumerate() {
        if is_cancelled() {
            return Err("Transcription annulée".into());
        }
        if end - start < MIN_CHUNK_SAMPLES {
            emit(i + 1);
            continue;
        }
        match run_chunk(loaded, &audio[start..end]) {
            Ok(text) => {
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            // OOM sur un gros morceau : une seule recoupe en deux, sans récursion.
            Err(e) if is_oom_error(&e) && end - start >= MIN_CHUNK_SAMPLES * 2 => {
                let mid = start + (end - start) / 2;
                let mut retried = Vec::new();
                let mut failed: Option<String> = None;
                for (s, e2) in [(start, mid), (mid, end)] {
                    if is_cancelled() {
                        return Err("Transcription annulée".into());
                    }
                    if e2 - s < MIN_CHUNK_SAMPLES {
                        continue;
                    }
                    match run_chunk(loaded, &audio[s..e2]) {
                        Ok(t) => {
                            if !t.is_empty() {
                                retried.push(t);
                            }
                        }
                        Err(e2) => {
                            failed = Some(e2);
                            break;
                        }
                    }
                }
                if let Some(err) = failed {
                    return Err(err);
                }
                parts.extend(retried);
            }
            Err(e) => return Err(e),
        }
        emit(i + 1);
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
    let threads = state
        .cpu_threads
        .lock()
        .map_err(|_| "Verrou cpu_threads indisponible".to_string())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, &mode, threads)?;
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
    let threads = state
        .cpu_threads
        .lock()
        .map_err(|_| "Verrou cpu_threads indisponible".to_string())?
        .clone();
    // Génération de cette transcription : `cancel_transcription` invalide les anciennes.
    let my_id = {
        let mut g = state
            .run_id
            .lock()
            .map_err(|_| "Verrou run_id indisponible".to_string())?;
        *g += 1;
        *g
    };
    let run_id = state.run_id.clone();
    // Durée max par morceau (réglable dans Paramètres), bornée pour rester raisonnable.
    let chunk_secs = chunk_secs.unwrap_or(MAX_CHUNK_SECS).clamp(5.0, 120.0);
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, &mode, threads)?;
        let loaded = guard.as_mut().ok_or("Aucun modèle chargé")?;
        transcribe_blocking(loaded, &wav, chunk_secs, &app, &|| {
            run_id.lock().map(|g| *g != my_id).unwrap_or(false)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Interrompt la transcription locale en cours après le morceau courant.
/// Sans effet s'il n'y en a pas (la prochaine transcription repart d'un id frais).
#[tauri::command]
pub async fn cancel_transcription(state: State<'_, LocalState>) -> Result<(), String> {
    *state
        .run_id
        .lock()
        .map_err(|_| "Verrou run_id indisponible".to_string())? += 1;
    Ok(())
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

/// Décharge le modèle local (libère la RAM). Si `model` est donné, ne décharge
/// que si c'est lui qui est chargé. Renvoie si un modèle était chargé.
#[tauri::command]
pub async fn unload_local_model(
    state: State<'_, LocalState>,
    model: Option<String>,
) -> Result<bool, String> {
    let slot = state.slot.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
        let mut guard = slot
            .lock()
            .map_err(|_| "Verrou du modèle indisponible".to_string())?;
        if let Some(name) = model {
            if guard.as_ref().is_some_and(|l| l.name != name) {
                return Ok(false);
            }
        }
        let was = guard.is_some();
        *guard = None;
        Ok(was)
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

/// Choisit le périphérique : "auto" (GPU si disponible, sinon CPU),
/// "gpu" (GPU requis), "cpu" (universel). Le modèle chargé est libéré pour que
/// le prochain chargement applique le nouveau choix.
#[tauri::command]
pub async fn set_local_device(state: State<'_, LocalState>, device: String) -> Result<(), String> {
    // Valide le mode
    let valid_modes = ["auto", "gpu", "cpu"];
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

/// Choisit les threads CPU des sessions ONNX : 0 = défaut ORT, sinon 1–64.
/// Le modèle chargé est libéré pour que le prochain chargement applique le nouveau choix.
#[tauri::command]
pub async fn set_cpu_threads(state: State<'_, LocalState>, threads: u32) -> Result<(), String> {
    if threads > 64 {
        return Err(format!("Nombre de threads invalide : {threads} (0 = auto, max 64)"));
    }
    *state
        .cpu_threads
        .lock()
        .map_err(|_| "Verrou cpu_threads indisponible".to_string())? =
        if threads == 0 { None } else { Some(threads as usize) };
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
