// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::Manager;
mod ctc;
mod local;
mod server;
mod fsops;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            server::test_server,
            server::transcribe_server
        ])
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
            local::set_local_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
