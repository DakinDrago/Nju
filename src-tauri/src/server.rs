//! Moteur « Serveur » : transcription via une API compatible OpenAI
//! (POST /v1/audio/transcriptions). Les requêtes partent de Rust, pas de la webview :
//! pas de CORS, clé API possible, délais et certificats maîtrisés.

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Client, RequestBuilder, multipart};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub insecure: bool,
}

fn base_url(cfg: &ServerConfig) -> String {
    cfg.url.trim().trim_end_matches('/').to_string()
}

fn build_client(cfg: &ServerConfig, timeout_secs: u64) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .danger_accept_invalid_certs(cfg.insecure)
        .build()
        .map_err(|e| e.to_string())
}

fn with_auth(req: RequestBuilder, cfg: &ServerConfig) -> RequestBuilder {
    match cfg.api_key.as_deref().map(str::trim) {
        Some(key) if !key.is_empty() => req.bearer_auth(key),
        _ => req,
    }
}

fn describe(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Délai dépassé : le serveur ne répond pas".into()
    } else if e.is_connect() {
        "Connexion impossible (serveur éteint ou URL incorrecte)".into()
    } else {
        e.to_string()
    }
}

pub async fn check_server(cfg: &ServerConfig) -> Result<String, String> {
    let client = build_client(cfg, 5)?;
    let base = base_url(cfg);
    let mut last = String::from("Aucune réponse");

    // /v1/models d'abord (protégé par la clé API sur les serveurs qui en exigent une)
    for path in ["/v1/models", "/health"] {
        match with_auth(client.get(format!("{base}{path}")), cfg)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => return Ok("Connecté".into()),
            Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
                return Err("Clé API refusée".into());
            }
            Ok(r) => last = format!("HTTP {} sur {}", r.status().as_u16(), path),
            Err(e) => return Err(describe(e)),
        }
    }
    Err(last)
}

pub async fn transcribe_with(cfg: &ServerConfig, audio: Vec<u8>) -> Result<String, String> {
    let part = multipart::Part::bytes(audio)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let model = cfg
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or("default")
        .to_string();

    let mut form = multipart::Form::new()
        .part("file", part)
        .text("model", model)
        .text("response_format", "json");

    if let Some(lang) = cfg
        .language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        form = form.text("language", lang.to_string());
    }

    let url = format!("{}/v1/audio/transcriptions", base_url(cfg));
    let resp = with_auth(build_client(cfg, 120)?.post(url), cfg)
        .multipart(form)
        .send()
        .await
        .map_err(describe)?;

    let status = resp.status();
    let body = resp.text().await.map_err(describe)?;

    if !status.is_success() {
        let extrait: String = body.chars().take(300).collect();
        return Err(format!("HTTP {} : {}", status.as_u16(), extrait));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| "Réponse invalide du serveur".to_string())?;
    json.get("text")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Réponse sans champ \"text\"".to_string())
}

#[tauri::command]
pub async fn test_server(config: ServerConfig) -> Result<String, String> {
    check_server(&config).await
}

#[tauri::command]
pub async fn transcribe_server(config: ServerConfig, audio_b64: String) -> Result<String, String> {
    let audio = STANDARD.decode(audio_b64).map_err(|e| e.to_string())?;
    transcribe_with(&config, audio).await
}
