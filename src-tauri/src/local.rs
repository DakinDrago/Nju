//! Moteur local : exécute un modèle wav2vec2/MMS exporté en ONNX, sans Python ni serveur.
//! Les modèles sont des sous-dossiers de `<données de l'appli>/models/` contenant
//! model.onnx (+ model.onnx.data), vocab.json et meta.json (produits par export_onnx.py).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
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
    /// true = « Auto » (GPU si disponible, sinon CPU) ; false = CPU imposé.
    prefer_gpu: Arc<AtomicBool>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
            prefer_gpu: Arc::new(AtomicBool::new(true)),
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

/// Crée la session ONNX. Le CPU fonctionne partout. Le GPU n'est tenté que si l'appli est compilée
/// avec `--features gpu` et que « Auto » est choisi ; en cas d'échec (pas de carte NVIDIA, bibliothèques
/// CUDA absentes...), on retombe sur le CPU et on le dit.
fn make_session(path: &Path, prefer_gpu: bool) -> Result<(Session, String), String> {
    let _ = prefer_gpu;
    #[allow(unused_mut)]
    let mut device = String::from("CPU");

    #[cfg(feature = "gpu")]
    {
        if prefer_gpu {
            let attempt = Session::builder()
                .and_then(|b| {
                    b.with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()])
                })
                .and_then(|mut b| b.commit_from_file(path));
            match attempt {
                Ok(session) => return Ok((session, "GPU (CUDA)".to_string())),
                Err(e) => {
                    let why: String = e.to_string().chars().take(120).collect();
                    device = format!("CPU (GPU indisponible : {why})");
                }
            }
        }
    }

    let mut builder = Session::builder().map_err(|e| e.to_string())?;
    let session = builder
        .commit_from_file(path)
        .map_err(|e| format!("Chargement ONNX impossible : {e}"))?;
    Ok((session, device))
}

fn load_blocking(dir: &Path, name: &str, prefer_gpu: bool) -> Result<Loaded, String> {
    for f in ["model.onnx", "vocab.json", "meta.json"] {
        if !dir.join(f).exists() {
            return Err(format!("Fichier manquant dans le dossier du modèle : {f}"));
        }
    }
    let meta_json = fs::read_to_string(dir.join("meta.json")).map_err(|e| e.to_string())?;
    let vocab_json = fs::read_to_string(dir.join("vocab.json")).map_err(|e| e.to_string())?;
    let meta = ctc::load_meta(&meta_json, &vocab_json)?;

    let (session, device) = make_session(&dir.join("model.onnx"), prefer_gpu)?;

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
    prefer_gpu: bool,
) -> Result<(), String> {
    if slot.as_ref().is_some_and(|l| l.name == name) {
        return Ok(());
    }
    *slot = None; // libère la mémoire de l'ancien modèle d'abord
    *slot = Some(load_blocking(dir, name, prefer_gpu)?);
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

fn transcribe_blocking(loaded: &mut Loaded, wav: &[u8]) -> Result<String, String> {
    let (samples, rate) = ctc::parse_wav(wav)?;
    let audio = ctc::resample(&samples, rate, loaded.meta.sample_rate);
    if audio.len() < MIN_CHUNK_SAMPLES {
        return Err("Audio trop court".into());
    }

    let mut parts = Vec::new();
    for (start, end) in ctc::split_chunks(&audio, loaded.meta.sample_rate, MAX_CHUNK_SECS) {
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
    let prefer_gpu = state.prefer_gpu.load(Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot.lock().map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, prefer_gpu)?;
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
) -> Result<String, String> {
    let dir = model_dir(&app, &model)?;
    let wav = STANDARD.decode(audio_b64).map_err(|e| e.to_string())?;
    let slot = state.slot.clone();
    let prefer_gpu = state.prefer_gpu.load(Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut guard = slot.lock().map_err(|_| "Verrou du modèle indisponible".to_string())?;
        ensure_loaded(&mut guard, &dir, &model, prefer_gpu)?;
        let loaded = guard.as_mut().ok_or("Aucun modèle chargé")?;
        transcribe_blocking(loaded, &wav)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Importe un dossier exporté (model.onnx, vocab.json, meta.json) dans le dossier des modèles.
/// mode = "copy" (copie le modèle dans l'appli) ou "link" (lien : le dossier d'origine doit rester en place).
#[tauri::command]
pub async fn import_local_model(app: AppHandle, source: String, mode: String) -> Result<String, String> {
    let src = PathBuf::from(&source);
    if !src.is_dir() || !fsops::has_model_files(&src) {
        return Err(
            "Ce dossier n'est pas un modèle exporté (model.onnx, vocab.json et meta.json attendus)".into(),
        );
    }
    let name = src
        .file_name()
        .ok_or("Nom de dossier invalide")?
        .to_string_lossy()
        .to_string();
    let dst = model_dir(&app, &name)?;
    if fs::symlink_metadata(&dst).is_ok() {
        return Err(format!("Un modèle nommé « {name} » existe déjà : retire-le d'abord"));
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
        let mut guard = slot.lock().map_err(|_| "Verrou du modèle indisponible".to_string())?;
        if guard.as_ref().is_some_and(|l| l.name == model) {
            *guard = None; // libère le fichier avant de le supprimer
        }
        fsops::remove_model(&dir)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Choisit le périphérique : "auto" (GPU si disponible) ou "cpu". Le modèle chargé est libéré
/// pour que le prochain chargement applique le nouveau choix.
#[tauri::command]
pub async fn set_local_device(state: State<'_, LocalState>, device: String) -> Result<(), String> {
    state.prefer_gpu.store(device != "cpu", Ordering::Relaxed);
    let slot = state.slot.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut guard = slot.lock().map_err(|_| "Verrou du modèle indisponible".to_string())?;
        *guard = None;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}