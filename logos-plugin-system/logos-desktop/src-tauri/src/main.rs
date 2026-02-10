// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use logos_core::Document;

#[tauri::command]
fn get_app_info() -> String {
    let doc = Document::default();
    format!("Logos Desktop v0.1.0 - Core ID: {}", doc.id)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
