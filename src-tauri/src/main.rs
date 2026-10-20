// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Contourne le crash au premier rendu de WebKitGTK (renderer DMABUF) sur
    // certaines configs Linux (NVIDIA + Ubuntu 24.04 notamment) : la fenêtre
    // s'ouvrait puis se fermait aussitôt. On n'écrase pas une valeur déjà
    // fournie par l'utilisateur. Doit être défini AVANT la création de la
    // webview, d'où ce bloc dans main() et pas dans setup().
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // Sûr : main() s'exécute en mono-thread, avant tout démarrage de
            // Tauri/webview (idem schéma unsafe déjà utilisé dans lib.rs).
            unsafe {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
    }
    to_nju_lib::run()
}
