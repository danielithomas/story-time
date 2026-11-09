//! Story Time: a desktop app for writing stories. All story logic lives in `story-time-core`;
//! this crate exposes it to the interface as Tauri commands.

// Release builds on Windows shouldn't open a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use story_time_core::config;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .setup(|app| {
            // Reopen the stories folder used last time, if it is still there.
            if let Ok(Some(root)) = config::user_default_vault()
                && root.is_dir()
            {
                app.state::<commands::AppState>().remember(root);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::inspect_folder,
            commands::default_bookshelf_place,
            commands::create_bookshelf,
            commands::open_vault,
            commands::upgrade_vault,
            commands::choices,
            commands::list_projects,
            commands::project_tree,
            commands::read_note,
            commands::save_note,
            commands::count_words,
            commands::create_project,
            commands::create_book,
            commands::add_note,
            commands::add_scene,
            commands::add_scrap,
            commands::add_research,
            commands::link_for,
            commands::writing_today,
            commands::record_snapshot,
            commands::book_progress,
            commands::check_project,
            commands::compile_book,
            commands::list_ideas,
            commands::capture_idea,
            commands::dev_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("Story Time failed to start");
}
