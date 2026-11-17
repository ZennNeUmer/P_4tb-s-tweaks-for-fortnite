use tauri_build::Attributes;

fn main() {
    // Подключаем манифест как ФАЙЛ через .rc (1 24 "path").
    // app_manifest() не подходит: tauri-winres оборачивает каждую строку
    // в " {} " и перед <?xml оказывается пробел -> SxS/XML-ошибка при старте.
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("windows-app-manifest.xml");
    let manifest_rc = format!(
        "1 24 \"{}\"",
        manifest_path.to_string_lossy().replace('\\', "/")
    );

    tauri_build::try_build(
        Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest()
                .append_rc_content(manifest_rc),
        ),
    )
    .expect("failed to run tauri-build");
}
