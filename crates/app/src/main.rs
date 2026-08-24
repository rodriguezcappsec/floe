mod appearance;
mod application;
mod browser;
pub mod copy_executor;
mod iconography;
pub mod job_manager;
mod launcher;
mod locations;
pub mod move_executor;
mod operations;
mod preferences;
pub mod state;
mod thumbnail;
mod thumbnail_cache;
pub mod trash_executor;
mod ui;
mod view;
mod worker;

fn main() -> gtk::glib::ExitCode {
    application::run()
}
