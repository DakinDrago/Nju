use std::{
    env,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn main() {
    // D'abord la lib ORT (pour que `bundle.resources` la trouve), puis Tauri.
    if let Err(e) = ensure_ort_dylib() {
        // Ne jamais casser le build (ex. hors-ligne) : l'absence de la lib
        // donne une erreur propre côté Rust au chargement du modèle.
        println!("cargo:warning=ONNX Runtime dylib not staged: {e}");
    }
    println!("cargo:rerun-if-changed=build.rs");
    tauri_build::build()
}

/// (URL archive, sha256, format, filtre de sous-dossier) par target rustc.
/// Plusieurs archives possibles (ex. Linux : base CPU + overlay WebGPU).
/// - Windows : NuGet MS DirectML (EP DirectML inclus) ;
/// - Linux : tgz MS officiel (base CPU) + overlay pyke +webgpu (provider WebGPU) ;
/// - macOS : tgz MS officiel (CoreML déjà inclus, vérifié dans la dylib).
/// Règle : CUDA seul est opt-in (`--features gpu`, dists `-gpu` NON embarquées) ;
/// chaque OS embarque son EP natif par défaut.
fn ort_dists() -> Option<Vec<(&'static str, &'static str, &'static str, Option<&'static str>)>> {
    let target = env::var("TARGET").unwrap_or_default();
    match target.as_str() {
        // Windows x64 : NuGet DirectML (onnxruntime.dll avec EP DirectML + providers_shared).
        "x86_64-pc-windows-msvc" => Some(vec![(
            "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/Microsoft.ML.OnnxRuntime.DirectML.1.22.0.nupkg",
            "b38d92c31d202d1dc772af9416fe30b6abae8527651e742c380c2d131e5f1817",
            "zip",
            Some("runtimes/win-x64/"),
        )]),
        "aarch64-pc-windows-msvc" => Some(vec![(
            "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/Microsoft.ML.OnnxRuntime.DirectML.1.22.0.nupkg",
            "b38d92c31d202d1dc772af9416fe30b6abae8527651e742c380c2d131e5f1817",
            "zip",
            Some("runtimes/win-arm64/"),
        )]),
        // Linux x64 : base MS officielle + overlay WebGPU pyke.
        // L'overlay +webgpu ne contient QUE libwebgpu_dawn.so (vérifié par
        // extraction) : sans la base, libonnxruntime.so manquerait.
        "x86_64-unknown-linux-gnu" => Some(vec![
            (
                "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-x64-1.22.0.tgz",
                "8344d55f93d5bc5021ce342db50f62079daf39aaafb5d311a451846228be49b3",
                "tgz",
                None,
            ),
            (
                "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.28.0/x86_64-unknown-linux-gnu+webgpu.tar.lzma2",
                "68406bc32de516ee8baeaa4c5f2de2bb0031269f19ce8b76d64988c0498b93be",
                "tar.lzma2",
                None,
            ),
        ]),
        // macOS ARM64 : tgz MS officiel (CoreMLExecutionProvider présent dans
        // la dylib, vérifié par inspection binaire).
        "aarch64-apple-darwin" => Some(vec![(
            "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-osx-arm64-1.22.0.tgz",
            "cab6dcbd77e7ec775390e7b73a8939d45fec3379b017c7cb74f5b204c1a1cc07",
            "tgz",
            None,
        )]),
        // Intel Mac : même pattern (CoreML présent aussi dans la dylib x86_64).
        "x86_64-apple-darwin" => Some(vec![(
            "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-osx-x86_64-1.22.0.tgz",
            "e4ec94a7696de74fb1b12846569aa94e499958af6ff7a186022cfde16c9d617f0",
            "tgz",
            None,
        )]),
        _ => None,
    }
}

/// Noms de libs attendus après staging, selon le TARGET (pas le HOST :
/// `cfg!` refléterait la machine qui compile, faux en cross-compilation).
/// Utilisé pour le court-circuit : si tout est déjà en place, on ne retélécharge rien.
fn ort_lib_names() -> Vec<&'static str> {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        vec!["onnxruntime.dll", "onnxruntime_providers_shared.dll"]
    } else if target.contains("apple-darwin") || target.contains("macos") {
        vec!["libonnxruntime.dylib"]
    } else {
        // Linux : base + provider WebGPU (overlay).
        vec!["libonnxruntime.so", "libwebgpu_dawn.so"]
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn is_lib_name(name: &str) -> bool {
    name.contains(".dll") || name.contains(".so") || name.contains(".dylib")
}

/// Télécharge (cache persisté) + extrait les libs partagées, puis les place :
/// - à côté de l'exécutable (dev / binaire direct : ort la trouve seul), et
/// - dans `ort-dylibs/` (embarquée via `bundle.resources` pour les paquets).
fn ensure_ort_dylib() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);

    let dists = ort_dists().ok_or_else(|| {
        format!(
            "no prebuilt ONNX Runtime archive pinned for target {}",
            env::var("TARGET").unwrap_or_default()
        )
    })?;

    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out → remonte au dossier du profil (à côté de l'exe).
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .ok_or("OUT_DIR inattendu")?
        .to_path_buf();
    let res_dir = manifest_dir.join("ort-dylibs");
    fs::create_dir_all(&res_dir).map_err(|e| e.to_string())?;

    // Court-circuit : libs attendues déjà en place aux deux endroits,
    // depuis les mêmes dists (stamp) ? Sinon on re-stage (changement de dist).
    let mut stamp = String::new();
    for (url, hash, _, _) in &dists {
        stamp.push_str(url);
        stamp.push('\n');
        stamp.push_str(hash);
        stamp.push('\n');
    }
    let wanted: Vec<PathBuf> = ort_lib_names().iter().map(PathBuf::from).collect();
    let same_stamp = |dir: &Path| {
        fs::read_to_string(dir.join(".dist-info"))
            .map(|s| s == stamp)
            .unwrap_or(false)
    };
    // Note : le stamp ne vit que côté profil (target/) car ort-dylibs/
    // est embarqué tel quel dans les paquets.
    let profile_ok = wanted.iter().all(|n| profile_dir.join(n).exists()) && same_stamp(&profile_dir);
    let res_ok = wanted.iter().all(|n| res_dir.join(n).exists());
    if profile_ok && res_ok {
        return Ok(());
    }

    // Cache persistant (survit à `cargo clean`), vérifié par hash.
    let cache_dir = manifest_dir.join(".ort-dylib-cache");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (url, hash, format, path_filter) in &dists {
        let url: &str = url;
        let hash: &str = hash;
        let format: &str = format;
        let archive_name = url.rsplit('/').next().unwrap_or("ort-archive").to_string();
        let archive_path = cache_dir.join(&archive_name);
        let need_download = match fs::read(&archive_path) {
            Ok(bytes) => sha256_hex(&bytes) != hash,
            Err(_) => true,
        };
        if need_download {
            let mut resp = ureq::get(url)
                .call()
                .map_err(|e| format!("download {url}: {e}"))?;
            let mut bytes = Vec::new();
            resp.body_mut()
                .as_reader()
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if sha256_hex(&bytes) != hash {
                return Err(format!("hash mismatch for {url}"));
            }
            fs::write(&archive_path, &bytes).map_err(|e| e.to_string())?;
        }
        let archive_bytes = fs::read(&archive_path).map_err(|e| e.to_string())?;
        if sha256_hex(&archive_bytes) != hash {
            return Err(format!(
                "cached archive failed hash check: {}",
                archive_path.display()
            ));
        }
        files.extend(extract_libs(&archive_bytes, format, url, *path_filter)?);
    }

    let place = |dir: &Path| -> Result<(), String> {
        for (name, bytes) in &files {
            let dest = dir.join(name);
            let same = fs::read(&dest).map(|b| b == *bytes).unwrap_or(false);
            if !same {
                fs::write(&dest, bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    };
    place(&profile_dir)?;
    place(&res_dir)?;
    // Mémorise la dist staged (côté profil uniquement : res_dir est embarqué
    // dans les paquets, on n'y met que la lib). Invalide si la dist change.
    fs::write(profile_dir.join(".dist-info"), &stamp).map_err(|e| e.to_string())?;
    Ok(())
}

/// Extrait les fichiers lib (.dll / .so* / .dylib) d'une archive .zip, .tgz
/// ou .tar.lzma2 (dists pyke), en suivant les symlinks par contenu
/// (copie simple, robuste partout).
fn extract_libs(
    archive_bytes: &[u8],
    format: &str,
    url: &str,
    path_filter: Option<&str>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    if format == "zip" {
        extract_zip(archive_bytes, path_filter)
    } else if format == "tgz" {
        let decoder = flate2::read::GzDecoder::new(archive_bytes);
        extract_tar(tar::Archive::new(decoder), path_filter)
    } else if format == "tar.lzma2" {
        let decoder = lzma_rust2::Lzma2Reader::new(archive_bytes, 1 << 26, None);
        extract_tar(tar::Archive::new(decoder), path_filter)
    } else {
        Err(format!("unsupported archive format for {url}"))
    }
}

fn entry_accepted(path: &Path, path_filter: Option<&str>) -> bool {
    let Some(f) = path_filter else {
        return true;
    };
    path.to_string_lossy().replace('\\', "/").contains(f)
}

fn extract_zip(archive_bytes: &[u8], path_filter: Option<&str>) -> Result<Vec<(String, Vec<u8>)>, String> {
    let reader = std::io::Cursor::new(archive_bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let full = entry.enclosed_name();
        if !full.as_ref().is_some_and(|p| entry_accepted(p, path_filter)) {
            continue;
        }
        let name = full
            .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string));
        let Some(name) = name else { continue };
        if !is_lib_name(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        files.push((name, bytes));
    }
    if files.is_empty() {
        return Err("no shared library found in archive".to_string());
    }
    Ok(files)
}

fn extract_tar<R: Read>(mut archive: tar::Archive<R>, path_filter: Option<&str>) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut links: Vec<(String, String)> = Vec::new();
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        if !entry_accepted(&path, path_filter) {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !is_lib_name(&name) {
            continue;
        }
        if entry.header().entry_type().is_symlink() {
            if let Ok(Some(target)) = entry.link_name() {
                if let Some(t) = target.to_str() {
                    links.push((name, t.to_string()));
                }
            }
            continue;
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        files.push((name, bytes));
    }
    // Résout les symlinks par contenu (copie simple, robuste partout).
    for (link_name, target) in &links {
        let target_file = target.rsplit('/').next().unwrap_or(target);
        if let Some((_, bytes)) = files.iter().find(|(n, _)| n == target_file) {
            let bytes = bytes.clone();
            if !files.iter().any(|(n, _)| n == link_name) {
                files.push((link_name.clone(), bytes));
            }
        }
    }
    if files.is_empty() {
        return Err("no shared library found in archive".to_string());
    }
    Ok(files)
}
