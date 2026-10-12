//! Opérations sur les dossiers de modèles (copie, lien, suppression), en std pur : testées à part.

use std::fs;
use std::path::Path;

pub const REQUIRED: [&str; 3] = ["model.onnx", "vocab.json", "meta.json"];

pub fn has_model_files(dir: &Path) -> bool {
    REQUIRED.iter().all(|f| dir.join(f).exists())
}

/// Un nom de modèle = un nom de dossier simple, ni caché ni avec chemin.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.contains("..")
}

/// Copie les fichiers du modèle dans un dossier temporaire caché, puis le renomme :
/// un modèle à moitié copié n'apparaît jamais dans la liste.
pub fn copy_model(src: &Path, root: &Path, name: &str, dst: &Path) -> Result<(), String> {
    let tmp = root.join(format!(".{name}.partial"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let result = (|| -> Result<(), String> {
        for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            if path.is_file() {
                fs::copy(&path, tmp.join(entry.file_name())).map_err(|e| {
                    format!("Copie de {} impossible : {e}", entry.file_name().to_string_lossy())
                })?;
            }
        }
        fs::rename(&tmp, dst).map_err(|e| e.to_string())
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result
}

#[cfg(unix)]
pub fn link_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(src, dst).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
pub fn link_dir(_src: &Path, _dst: &Path) -> Result<(), String> {
    Err("Les liens ne sont pas pris en charge ici : utilise « Copier ».".into())
}

/// Supprime un modèle : pour un lien, seul le lien disparaît (jamais le dossier d'origine).
pub fn remove_model(dir: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    let result = if meta.file_type().is_symlink() {
        fs::remove_file(dir)
    } else {
        fs::remove_dir_all(dir)
    };
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nju-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_model(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        for f in REQUIRED {
            fs::write(dir.join(f), f.as_bytes()).unwrap();
        }
        fs::write(dir.join("model.onnx.data"), vec![7u8; 10_000]).unwrap();
    }

    #[test]
    fn noms_valides_et_invalides() {
        assert!(valid_name("mms-fmp"));
        assert!(!valid_name(""));
        assert!(!valid_name(".cache"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("a\\b"));
        assert!(!valid_name(".."));
    }

    #[test]
    fn detecte_un_dossier_de_modele() {
        let t = tmpdir("detect");
        assert!(!has_model_files(&t));
        fake_model(&t.join("m"));
        assert!(has_model_files(&t.join("m")));
        fs::remove_file(t.join("m/vocab.json")).unwrap();
        assert!(!has_model_files(&t.join("m")));
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn copie_tous_les_fichiers_sans_laisser_de_dossier_temporaire() {
        let t = tmpdir("copy");
        let (src, root) = (t.join("source/mms-fmp"), t.join("models"));
        fake_model(&src);
        fs::create_dir_all(&root).unwrap();
        let dst = root.join("mms-fmp");

        copy_model(&src, &root, "mms-fmp", &dst).unwrap();

        assert!(has_model_files(&dst));
        assert_eq!(fs::metadata(dst.join("model.onnx.data")).unwrap().len(), 10_000);
        assert!(!root.join(".mms-fmp.partial").exists());
        assert!(src.exists(), "la source doit rester intacte");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn copie_echoue_proprement_si_la_source_est_absente() {
        let t = tmpdir("copyfail");
        let root = t.join("models");
        fs::create_dir_all(&root).unwrap();
        let r = copy_model(&t.join("nulle-part"), &root, "x", &root.join("x"));
        assert!(r.is_err());
        assert!(!root.join(".x.partial").exists());
        assert!(!root.join("x").exists());
        let _ = fs::remove_dir_all(&t);
    }

    #[cfg(unix)]
    #[test]
    fn lien_puis_suppression_ne_touche_pas_la_source() {
        let t = tmpdir("link");
        let (src, root) = (t.join("source/mms-fmp"), t.join("models"));
        fake_model(&src);
        fs::create_dir_all(&root).unwrap();
        let dst = root.join("mms-fmp");

        link_dir(&src, &dst).unwrap();
        assert!(has_model_files(&dst), "le lien doit donner accès aux fichiers");

        remove_model(&dst).unwrap();
        assert!(fs::symlink_metadata(&dst).is_err(), "le lien est supprimé");
        assert!(has_model_files(&src), "la source est intacte");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn suppression_d_une_copie_supprime_le_dossier() {
        let t = tmpdir("rm");
        fake_model(&t.join("m"));
        remove_model(&t.join("m")).unwrap();
        assert!(!t.join("m").exists());
        assert!(remove_model(&t.join("m")).is_err());
        let _ = fs::remove_dir_all(&t);
    }
}