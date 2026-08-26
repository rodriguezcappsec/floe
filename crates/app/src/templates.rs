use std::{
    ffi::OsString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use adw::prelude::*;
use thiserror::Error;

pub const TEMPLATE_CATALOG_CAPACITY: usize = 256;
pub const TEMPLATE_SCAN_CAPACITY: usize = 4_096;
const TEMPLATE_REQUEST_CAPACITY: usize = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateEntry {
    path: PathBuf,
    name: OsString,
}

impl TemplateEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &OsString {
        &self.name
    }

    pub fn display_name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateCatalog {
    root: PathBuf,
    entries: Vec<TemplateEntry>,
    truncated: bool,
}

impl TemplateCatalog {
    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[TemplateEntry] {
        &self.entries
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Debug, Error)]
pub enum TemplateDiscoveryError {
    #[error("Templates folder is unavailable")]
    Unavailable,
    #[error("could not inspect Templates folder: {0}")]
    Io(#[source] io::Error),
}

pub fn discover_templates(root: &Path) -> Result<TemplateCatalog, TemplateDiscoveryError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            TemplateDiscoveryError::Unavailable
        } else {
            TemplateDiscoveryError::Io(error)
        }
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(TemplateDiscoveryError::Unavailable);
    }

    let directory = fs::read_dir(root).map_err(TemplateDiscoveryError::Io)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for result in directory.take(TEMPLATE_SCAN_CAPACITY + 1) {
        if entries.len() == TEMPLATE_CATALOG_CAPACITY {
            truncated = true;
            break;
        }
        let entry = result.map_err(TemplateDiscoveryError::Io)?;
        let path = entry.path();
        let entry_metadata = fs::symlink_metadata(&path).map_err(TemplateDiscoveryError::Io)?;
        if !entry_metadata.file_type().is_file() || entry_metadata.file_type().is_symlink() {
            continue;
        }
        entries.push(TemplateEntry {
            name: entry.file_name(),
            path,
        });
    }
    entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));

    Ok(TemplateCatalog {
        root: root.to_path_buf(),
        entries,
        truncated,
    })
}

#[derive(Clone, Debug)]
struct TemplateRequest {
    id: u64,
    root: PathBuf,
}

#[derive(Debug)]
pub struct TemplateResponse {
    pub id: u64,
    pub result: Result<TemplateCatalog, TemplateDiscoveryError>,
}

#[derive(Debug, Error)]
pub enum TemplateSubmitError {
    #[error("template discovery is already busy")]
    Busy,
    #[error("template discovery worker stopped")]
    Stopped,
}

pub struct TemplateWorker {
    sender: Option<SyncSender<TemplateRequest>>,
    receiver: Receiver<TemplateResponse>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct TemplateDialogWidgets {
    pub dialog: adw::Dialog,
    pub list: gtk::ListBox,
    pub status: gtk::Label,
    pub spinner: gtk::Spinner,
    pub open_folder_button: gtk::Button,
}

pub fn build_template_dialog() -> TemplateDialogWidgets {
    let heading = gtk::Label::builder()
        .label("Create from a template")
        .halign(gtk::Align::Start)
        .build();
    heading.add_css_class("title-2");

    let description = gtk::Label::builder()
        .label("Choose a file from your XDG Templates folder. The new copy will not be executable.")
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .build();
    description.add_css_class("dim-label");

    let spinner = gtk::Spinner::builder().halign(gtk::Align::Center).build();
    spinner.start();
    let status = gtk::Label::builder()
        .label("Loading templates…")
        .halign(gtk::Align::Center)
        .wrap(true)
        .build();

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .activate_on_single_click(true)
        .build();
    list.add_css_class("boxed-list");
    list.set_visible(false);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();

    let open_folder_button = gtk::Button::with_label("Open Templates Folder");
    open_folder_button.set_halign(gtk::Align::End);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&heading);
    content.append(&description);
    content.append(&spinner);
    content.append(&status);
    content.append(&scrolled);
    content.append(&open_folder_button);

    let dialog = adw::Dialog::builder()
        .title("Choose Template")
        .content_width(560)
        .content_height(520)
        .child(&content)
        .build();
    dialog.update_property(&[gtk::accessible::Property::Label("Choose Template")]);

    TemplateDialogWidgets {
        dialog,
        list,
        status,
        spinner,
        open_folder_button,
    }
}

impl TemplateWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<TemplateRequest>(TEMPLATE_REQUEST_CAPACITY);
        let (responses, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("floe-template-worker".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let response = TemplateResponse {
                        id: request.id,
                        result: discover_templates(&request.root),
                    };
                    if responses.send(response).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            worker: Some(worker),
        })
    }

    pub fn request(&self, id: u64, root: PathBuf) -> Result<(), TemplateSubmitError> {
        let Some(sender) = &self.sender else {
            return Err(TemplateSubmitError::Stopped);
        };
        match sender.try_send(TemplateRequest { id, root }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(TemplateSubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(TemplateSubmitError::Stopped),
        }
    }

    pub fn try_response(&self) -> Option<TemplateResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for TemplateWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("template discovery worker panicked during shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStringExt, os::unix::fs as unix_fs};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_12d_templates_are_bounded_sorted_and_no_follow() {
        let fixture = tempdir().expect("temporary fixture");
        let raw_name = OsString::from_vec(b"a-\xff.txt".to_vec());
        let raw_path = fixture.path().join(&raw_name);
        fs::write(&raw_path, b"raw template").expect("raw template");
        fs::write(fixture.path().join("z.txt"), b"last template").expect("last template");
        fs::create_dir(fixture.path().join("folder")).expect("nested folder");
        unix_fs::symlink(&raw_path, fixture.path().join("linked-template"))
            .expect("template symlink");

        let catalog = discover_templates(fixture.path()).expect("template catalog");
        assert_eq!(catalog.root(), fixture.path());
        assert_eq!(catalog.entries().len(), 2);
        assert_eq!(catalog.entries()[0].name().as_bytes(), raw_name.as_bytes());
        assert_eq!(catalog.entries()[0].path(), raw_path);
        assert_eq!(catalog.entries()[1].name(), "z.txt");
        assert!(!catalog.truncated());
    }

    #[test]
    fn phase_12d_templates_catalog_caps_results_and_worker_returns_exact_root() {
        let fixture = tempdir().expect("temporary fixture");
        for index in 0..=TEMPLATE_CATALOG_CAPACITY {
            fs::write(
                fixture.path().join(format!("template-{index:03}")),
                b"template",
            )
            .expect("template fixture");
        }

        let worker = TemplateWorker::spawn().expect("template worker");
        worker
            .request(7, fixture.path().to_path_buf())
            .expect("template request");
        let response = loop {
            if let Some(response) = worker.try_response() {
                break response;
            }
            std::thread::yield_now();
        };
        assert_eq!(response.id, 7);
        let catalog = response.result.expect("template response");
        assert_eq!(catalog.entries().len(), TEMPLATE_CATALOG_CAPACITY);
        assert!(catalog.truncated());
        assert_eq!(catalog.root(), fixture.path());
    }

    #[test]
    fn phase_12d_templates_unavailable_root_is_explicit() {
        let fixture = tempdir().expect("temporary fixture");
        let missing = fixture.path().join("missing");
        assert!(matches!(
            discover_templates(&missing),
            Err(TemplateDiscoveryError::Unavailable)
        ));
    }
}
