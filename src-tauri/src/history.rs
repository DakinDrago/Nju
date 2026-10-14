//! Historique des transcriptions : stockage, correction, export MMS-1B-all (TSV)

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

const SPEAKER_ID: &str = "speaker_001";
const LANGUAGE: &str = "fmp";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transcription {
    pub id: String,
    pub timestamp: String,  // ISO 8601
    pub engine: String,     // ex: "local:mms-fmp" ou "server:mon-serveur"
    pub audio_file: String, // chemin relatif : "audio/uuid.wav"
    pub original_text: String,
    #[serde(default)]
    pub corrected_text: String, // vide = pas encore corrigé
    pub duration_sec: f32,
    pub sample_rate: u32,
    #[serde(default)]
    pub status: TranscriptionStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionStatus {
    Original,  // texte original, pas corrigé
    Pending,   // en attente de correction
    Corrected, // corrigé par l'utilisateur
}

impl Default for TranscriptionStatus {
    fn default() -> Self {
        TranscriptionStatus::Original
    }
}

pub struct HistoryState {
    index_path: PathBuf,
    audio_dir: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl HistoryState {
    fn new(app: &AppHandle) -> Result<Self, String> {
        let base = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("history");
        let audio_dir = base.clone(); // audio_dir pointe vers la racine history/
        let index_path = base.join("index.json");

        // Crée le dossier de base et le sous-dossier audio
        fs::create_dir_all(&base)
            .map_err(|e| format!("Impossible de créer le dossier history: {e}"))?;
        let audio_subdir = base.join("audio");
        fs::create_dir_all(&audio_subdir)
            .map_err(|e| format!("Impossible de créer le dossier audio: {e}"))?;

        // Vérifie que les dossiers existent vraiment
        if !base.exists() {
            return Err(format!("Dossier history non créé: {}", base.display()));
        }
        if !audio_subdir.exists() {
            return Err(format!(
                "Dossier audio non créé: {}",
                audio_subdir.display()
            ));
        }

        if !index_path.exists() {
            fs::write(&index_path, "[]").map_err(|e| e.to_string())?;
        }
        Ok(Self {
            index_path,
            audio_dir,
            lock: Arc::new(Mutex::new(())),
        })
    }

    fn load_index(&self) -> Result<Vec<Transcription>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Verrou historique indisponible".to_string())?;
        let content = fs::read_to_string(&self.index_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    fn save_index(&self, items: &[Transcription]) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Verrou historique indisponible".to_string())?;
        let content = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
        fs::write(&self.index_path, content).map_err(|e| e.to_string())
    }
}

/// Sauvegarde une nouvelle transcription (appelée depuis l'UI après transcription)
#[tauri::command]
pub async fn save_transcription(
    _app: AppHandle,
    state: State<'_, HistoryState>,
    audio_b64: String,
    original_text: String,
    engine: String,
    duration_sec: f32,
    sample_rate: u32,
    extension: Option<String>,
) -> Result<String, String> {
    let history = state.inner();
    let id = uuid::Uuid::new_v4().to_string();
    // Extension du fichier d'origine (opus par défaut pour les enregistrements micro)
    let ext = extension
        .unwrap_or_else(|| "opus".to_string())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>();
    let ext = if ext.is_empty() {
        "opus".to_string()
    } else {
        ext
    };
    let audio_file = format!("audio/{id}.{ext}");
    let audio_path = history.audio_dir.join(&audio_file);

    // S'assure que le dossier audio existe
    let audio_subdir = history.audio_dir.join("audio");
    if !audio_subdir.exists() {
        fs::create_dir_all(&audio_subdir)
            .map_err(|e| format!("Création dossier audio échouée: {e}"))?;
    }

    // Décode et sauve l'audio (OPUS dans conteneur WebM)
    let audio_bytes = STANDARD.decode(&audio_b64).map_err(|e| e.to_string())?;
    fs::write(&audio_path, audio_bytes).map_err(|e| format!("Écriture audio : {e}"))?;

    let transcription = Transcription {
        id: id.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        engine,
        audio_file,
        original_text,
        corrected_text: String::new(),
        duration_sec,
        sample_rate,
        status: TranscriptionStatus::Original,
    };

    let mut items = history.load_index()?;
    items.insert(0, transcription); // plus récent en premier
    history.save_index(&items)?;

    Ok(id)
}

/// Liste l'historique (paginé)
#[tauri::command]
pub async fn list_history(
    state: State<'_, HistoryState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<Transcription>, String> {
    let history = state.inner();
    let mut items = history.load_index()?;
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);
    items.drain(..offset.min(items.len())).for_each(drop);
    items.truncate(limit);
    Ok(items)
}

/// Récupère une transcription complète (pour édition)
#[tauri::command]
pub async fn get_transcription(
    state: State<'_, HistoryState>,
    id: String,
) -> Result<Option<Transcription>, String> {
    let history = state.inner();
    let items = history.load_index()?;
    Ok(items.into_iter().find(|t| t.id == id))
}

/// Met à jour le texte corrigé
#[tauri::command]
pub async fn update_transcription(
    state: State<'_, HistoryState>,
    id: String,
    corrected_text: String,
) -> Result<(), String> {
    let history = state.inner();
    let mut items = history.load_index()?;
    if let Some(t) = items.iter_mut().find(|t| t.id == id) {
        t.corrected_text = corrected_text.clone();
        t.status = if corrected_text.trim().is_empty() {
            TranscriptionStatus::Pending
        } else {
            TranscriptionStatus::Corrected
        };
        history.save_index(&items)?;
        Ok(())
    } else {
        Err("Transcription non trouvée".into())
    }
}

/// Supprime une transcription (audio + index)
#[tauri::command]
pub async fn delete_transcription(
    _app: AppHandle,
    state: State<'_, HistoryState>,
    id: String,
) -> Result<(), String> {
    let history = state.inner();
    let mut items = history.load_index()?;
    if let Some(pos) = items.iter().position(|t| t.id == id) {
        let t = items.remove(pos);
        let audio_path = history.audio_dir.join(&t.audio_file);
        eprintln!(
            "DEBUG delete: id={}, audio_file={}, audio_path={}",
            id,
            t.audio_file,
            audio_path.display()
        );
        if audio_path.exists() {
            eprintln!("DEBUG delete: file exists, deleting...");
            fs::remove_file(audio_path).ok();
        } else {
            eprintln!("DEBUG delete: file does NOT exist at path");
        }
        history.save_index(&items)?;
        Ok(())
    } else {
        Err("Transcription non trouvée".into())
    }
}

/// Exporte au format MMS-1B-all (TSV) + copie les audios.
/// `language` / `speaker` : valeurs réglables dans Paramètres (défauts : "fmp" / "speaker_001").
#[tauri::command]
pub async fn export_mms_tsv(
    _app: AppHandle,
    state: State<'_, HistoryState>,
    output_dir: String,
    corrected_only: Option<bool>,
    language: Option<String>,
    speaker: Option<String>,
) -> Result<String, String> {
    let history = state.inner();
    let items = history.load_index()?;

    let corrected_only = corrected_only.unwrap_or(true);
    let language = language
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| LANGUAGE.to_string());
    let speaker = speaker
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SPEAKER_ID.to_string());

    let filtered: Vec<_> = items
        .into_iter()
        .filter(|t| !corrected_only || t.status == TranscriptionStatus::Corrected)
        .collect();

    if filtered.is_empty() {
        return Err("Aucune transcription à exporter".into());
    }

    let out_base = PathBuf::from(&output_dir);
    let export_dir = out_base.join(format!(
        "export_mms_{}",
        chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S")
    ));
    let audio_out_dir = export_dir.join("audio");
    fs::create_dir_all(&audio_out_dir).map_err(|e| e.to_string())?;

    // Copie les fichiers audio
    for t in &filtered {
        let src = history.audio_dir.join(&t.audio_file);
        let dst = audio_out_dir.join(&t.audio_file);
        if src.exists() {
            eprintln!("DEBUG export: copying audio file {id}", id = t.id);
            fs::copy(&src, &dst).map_err(|e| format!("Copie audio {id} : {e}", id = t.id))?;
        }
    }

    // Génère le TSV
    let tsv_path = export_dir.join("dataset.tsv");
    let mut tsv_content = String::from("path\ttext\tlanguage\tspeaker_id\tduration\n");
    for t in &filtered {
        let text =
            if t.status == TranscriptionStatus::Corrected && !t.corrected_text.trim().is_empty() {
                &t.corrected_text
            } else {
                &t.original_text
            };
        let text_escaped = text.replace('\t', " ").replace('\n', " ").replace('\r', "");
        tsv_content.push_str(&format!(
            "{}\t{}\t{}\t{}\t{:.3}\n",
            t.audio_file, text_escaped, language, speaker, t.duration_sec
        ));
    }
    fs::write(&tsv_path, tsv_content).map_err(|e| e.to_string())?;

    // README
    let total_duration: f32 = filtered.iter().map(|t| t.duration_sec).sum();
    let readme = format!(
        r#"# Export MMS-1B-all - {}

- Échantillons : {}
- Durée totale : {:.1}s ({:.1} min)
- Langue : {}
- Locuteur : {}
- Filtre : {}

Généré le {} par Njū'
"#,
        export_dir.file_name().unwrap().to_string_lossy(),
        filtered.len(),
        total_duration,
        total_duration / 60.0,
        language,
        speaker,
        if corrected_only {
            "corrigées uniquement"
        } else {
            "toutes"
        },
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    fs::write(export_dir.join("README.md"), readme).map_err(|e| e.to_string())?;

    Ok(export_dir.display().to_string())
}

/// Initialise l'état historique (appelé au démarrage)
pub fn init_history(app: &AppHandle) -> Result<HistoryState, String> {
    HistoryState::new(app)
}

/// Lit un fichier audio et le retourne en base64 (pour lecture dans le navigateur)
#[tauri::command]
pub async fn read_audio_file(
    state: State<'_, HistoryState>,
    audio_file: String,
) -> Result<String, String> {
    let history = state.inner();
    let audio_path = history.audio_dir.join(&audio_file);
    if !audio_path.exists() {
        return Err("Fichier audio introuvable".into());
    }
    let bytes = fs::read(audio_path).map_err(|e| e.to_string())?;
    Ok(STANDARD.encode(bytes))
}
