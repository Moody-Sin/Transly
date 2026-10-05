use specta_typescript::Typescript;
use src_tauri::commands;
use tauri_specta::{collect_commands, Builder};

fn main() {
    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::translate,
        commands::get_engine_capabilities,
        commands::detect_lang,
        commands::get_server_token,
        commands::rotate_server_token,
        commands::save_api_key,
        commands::is_api_key_configured,
        commands::delete_api_key,
        commands::add_glossary_term,
        commands::get_glossary_terms,
        commands::delete_glossary_term,
        commands::clear_cache,
        commands::set_cache_enabled,
        commands::get_cache_enabled,
        commands::ocr_image,
        commands::set_clipboard_monitor,
        commands::get_clipboard_monitor
    ]);

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let output_path = std::path::Path::new(&manifest_dir).join("../src/bindings.ts");

    builder
        .export(Typescript::default(), &output_path)
        .expect("Failed to export typescript bindings");

    println!(
        "TypeScript bindings successfully generated at {:?}",
        output_path
    );
}
