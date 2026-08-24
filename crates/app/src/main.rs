mod appearance;
mod application;
mod browser;
pub mod copy_executor;
pub mod job_manager;
mod launcher;
mod locations;
pub mod move_executor;
mod operations;
mod preferences;
pub mod state;
mod thumbnail;
pub mod trash_executor;
mod ui;
mod view;
mod worker;

fn main() -> gtk::glib::ExitCode {
    application::run()
}
