use specta_typescript::Typescript;

fn main() {
    let builder = src_tauri::app_command_builder!();

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
