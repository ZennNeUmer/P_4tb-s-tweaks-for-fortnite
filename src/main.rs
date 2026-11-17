#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tweaks;

#[tauri::command]
fn list_tweaks() -> Vec<tweaks::TweakInfo> {
    tweaks::list_tweaks()
}

#[tauri::command]
fn apply_tweak(id: String, enable: bool) -> Result<String, String> {
    tweaks::apply_tweak(&id, enable)?;
    let state = match tweaks::tweak_state(&id) {
        Some(true) => "применён",
        Some(false) => "сброшен",
        None => "изменён",
    };
    Ok(format!("Тик {id} {state}"))
}

#[tauri::command]
fn apply_all(enable: bool) -> Vec<tweaks::TweakResult> {
    tweaks::apply_all(enable)
}

#[tauri::command]
fn is_admin() -> bool {
    tweaks::is_admin()
}

#[tauri::command]
fn restart_explorer() -> Result<String, String> {
    tweaks::restart_explorer()?;
    Ok("Проводник перезапущен".to_string())
}

#[tauri::command]
fn create_restore_point() -> Result<String, String> {
    tweaks::create_restore_point()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_tweaks,
            apply_tweak,
            apply_all,
            is_admin,
            restart_explorer,
            create_restore_point,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
