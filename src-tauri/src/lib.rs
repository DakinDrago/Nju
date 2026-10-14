// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
mod ctc;
mod fsops;
mod history;
mod local;
mod options;
mod server;

/// Raccourci global (phase 1) : Ctrl+Maj+Espace = démarrer/arrêter la transcription.
/// Affiché dans l'interface (page Paramètres) ; presse-papiers pour coller ensuite.
const TOGGLE_SHORTCUT: &str = "Ctrl+Shift+Space";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Instance unique : un 2e lancement réveille la fenêtre existante
        // (même cachée en tray) au lieu d'un 2e process.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            server::test_server,
            server::transcribe_server
        ])
        .manage(options::AppOptions::default())
        .setup(|app| {
            #[cfg(target_os = "linux")]
            {
                use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};

                let window = app.get_webview_window("main").unwrap();
                window.with_webview(|webview| {
                    let wv = webview.inner();
                    if let Some(settings) = WebViewExt::settings(&wv) {
                        settings.set_enable_media_stream(true);
                    }
                    wv.connect_permission_request(|_, request| {
                        request.allow();
                        true
                    });
                })?;
            }
            // Initialise l'historique
            let history_state = history::init_history(app.handle())?;
            app.manage(history_state);

            // Pointe ONNX Runtime vers la lib embarquée dans les paquets
            // (AppImage/deb/dmg/msi via bundle.resources). En dev, la lib est
            // à côté de l'exécutable (voir build.rs) et ort la trouve seul :
            // on ne touche à rien sauf si la ressource existe vraiment.
            // Doit arriver AVANT tout usage d'ort (chargement paresseux).
            if std::env::var("ORT_DYLIB_PATH")
                .unwrap_or_default()
                .is_empty()
            {
                if let Ok(res_dir) = app.path().resource_dir() {
                    let name = if cfg!(target_os = "windows") {
                        "onnxruntime.dll"
                    } else if cfg!(target_os = "macos") {
                        "libonnxruntime.dylib"
                    } else {
                        "libonnxruntime.so"
                    };
                    let candidate = res_dir.join("ort-dylibs").join(name);
                    if candidate.exists() {
                        // Sûr ici : setup() tourne avant tout thread utilisant l'env.
                        unsafe { std::env::set_var("ORT_DYLIB_PATH", &candidate) };
                    }
                }
            }

            // Raccourci global par défaut (l'interface applique ensuite les réglages
            // sauvegardés via `set_hotkey`). Échec non bloquant (Wayland, conflit...).
            if let Err(e) =
                app.global_shortcut()
                    .on_shortcut(TOGGLE_SHORTCUT, |app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            let _ = app.emit("toggle-recording", ());
                        }
                    })
            {
                eprintln!("Raccourci global '{TOGGLE_SHORTCUT}' non enregistré : {e}");
            }

            // Icône de notification : l'appli reste en fond, fermer = masquer.
            let icon_bytes = include_bytes!("../icons/32x32.png");
            let icon = tauri::image::Image::from_bytes(icon_bytes)?;
            let show = tauri::menu::MenuItem::with_id(app, "show", "Afficher", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            tauri::tray::TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Njū' — transcription")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Fermer la fenêtre = la masquer si « rester en fond » est actif
            // (l'appli continue via le tray), sinon quitter. Réglable dans Paramètres.
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let background = app_handle
                            .state::<options::AppOptions>()
                            .run_in_background
                            .lock()
                            .map(|v| *v)
                            .unwrap_or(true);
                        if background {
                            api.prevent_close();
                            let _ = w.hide();
                        } else {
                            app_handle.exit(0);
                        }
                    }
                });
            }
            Ok(())
        })
        .manage(local::LocalState::default())
        .invoke_handler(tauri::generate_handler![
            server::test_server,
            server::transcribe_server,
            local::models_dir,
            local::list_local_models,
            local::load_local_model,
            local::transcribe_local,
            local::import_local_model,
            local::remove_local_model,
            local::set_local_device,
            history::save_transcription,
            history::list_history,
            history::get_transcription,
            history::update_transcription,
            history::delete_transcription,
            history::export_mms_tsv,
            history::read_audio_file,
            options::set_hotkey,
            options::set_run_in_background,
            options::get_capabilities
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
