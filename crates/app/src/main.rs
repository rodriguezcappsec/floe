mod appearance;
mod application;
mod bookmarks;
mod browser;
mod clipboard;
pub mod copy_executor;
pub mod create_executor;
mod devices;
mod drag_drop;
mod file_watcher;
mod iconography;
pub mod job_manager;
mod launcher;
mod location_input;
mod locations;
pub mod move_executor;
mod operation_control;
mod operations;
mod permanent_delete_executor;
mod preferences;
pub mod restore_executor;
pub mod state;
mod system_thumbnailer;
mod thumbnail;
mod thumbnail_cache;
pub mod trash_executor;
mod ui;
mod view;
mod worker;

fn main() -> gtk::glib::ExitCode {
    application::run()
}
