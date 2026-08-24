mod appearance;
mod application;
mod browser;
pub mod copy_executor;
pub mod job_manager;
mod launcher;
mod locations;
pub mod move_executor;
mod operations;
pub mod state;
pub mod trash_executor;
mod ui;
mod worker;

fn main() -> gtk::glib::ExitCode {
    application::run()
}
