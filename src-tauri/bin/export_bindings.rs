use specta_typescript::Typescript;
use src_tauri::commands;
use tauri_specta::{collect_commands, Builder};

fn main() {
    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::translate,
        commands::detect_lang,
        commands::get_server_token,
        commands::save_api_key,
        commands::get_api_key,
        commands::delete_api_key,
        commands::add_glossary_term
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
