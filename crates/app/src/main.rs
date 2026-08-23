mod appearance;
mod application;
mod browser;
pub mod copy_executor;
pub mod job_manager;
mod launcher;
mod locations;
pub mod state;
mod ui;
mod worker;

fn main() -> gtk::glib::ExitCode {
    application::run()
}
