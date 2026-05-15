#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sandbox_core::sandbox::{Sandbox, SandboxConfig, SandboxState};
use std::sync::Mutex;

struct AppState {
    sandbox: Mutex<Sandbox>,
}

#[tauri::command]
fn get_sandbox_state(state: tauri::State<AppState>) -> Result<SandboxState, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    Ok(sandbox.state().clone())
}

#[tauri::command]
fn take_screenshot(state: tauri::State<AppState>) -> Result<Vec<u8>, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    sandbox.screenshot().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_sandbox_config(state: tauri::State<AppState>) -> Result<SandboxConfig, String> {
    let sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    Ok(sandbox.config().clone())
}

#[tauri::command]
fn init_sandbox(state: tauri::State<AppState>, window_id: u32) -> Result<(), String> {
    let mut sandbox = state.sandbox.lock().map_err(|e| e.to_string())?;
    sandbox.init(window_id).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            sandbox: Mutex::new(Sandbox::new(SandboxConfig::default())),
        })
        .invoke_handler(tauri::generate_handler![
            get_sandbox_state,
            take_screenshot,
            get_sandbox_config,
            init_sandbox,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
