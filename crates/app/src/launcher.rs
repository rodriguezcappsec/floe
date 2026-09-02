use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use gtk::{
    gio::{self, prelude::*},
    glib,
};
use thiserror::Error;

pub enum DefaultLaunch {
    Launched,
    NoDefault(OpenWithOptions),
}

#[cfg(test)]
mod phase_19b_association_tests {
    use super::*;

    #[test]
    fn phase_19b_association_requests_are_bounded_and_explicit() {
        assert!(
            AssociationChange::Reset {
                content_type: "application/pdf".to_owned()
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            AssociationChange::Reset {
                content_type: String::new()
            }
            .validate(),
            Err(AssociationChangeError::InvalidRequest)
        );
        assert_eq!(
            AssociationChange::SetDefault {
                content_type: "text/plain".to_owned(),
                application_id: String::new(),
                application_name: "Editor".to_owned(),
            }
            .validate(),
            Err(AssociationChangeError::InvalidRequest)
        );
    }

    #[test]
    fn phase_19b_association_missing_application_fails_without_mutation() {
        let change = AssociationChange::SetDefault {
            content_type: "application/x-floe-test-never-registered".to_owned(),
            application_id: "io.github.floe.NoSuchApplication.desktop".to_owned(),
            application_name: "Missing".to_owned(),
        };
        assert_eq!(
            apply_association_change(&change),
            Err(AssociationChangeError::MissingApplication)
        );
    }
}

/// Resolves and opens a local path with its registered default application.
/// When no default exists, the already-discovered compatible applications are
/// returned to the caller for an explicit chooser instead of becoming an error.
pub fn launch_default(
    path: &Path,
    callback: impl FnOnce(Result<DefaultLaunch, glib::Error>) + 'static,
) {
    let path = path.to_path_buf();
    glib::spawn_future_local(async move {
        let options = match discover_open_with(path).await {
            Ok(options) => options,
            Err(error) => {
                callback(Err(error));
                return;
            }
        };

        let Some(application) = default_application(&options) else {
            callback(Ok(DefaultLaunch::NoDefault(options)));
            return;
        };
        let launch_path = options.path.clone();
        launch_with(&application, &launch_path, move |result| {
            callback(result.map(|()| DefaultLaunch::Launched));
        });
    });
}

#[derive(Clone)]
pub struct OpenWithApplication {
    pub app_info: gio::AppInfo,
    pub display_name: String,
    pub is_default: bool,
}

pub struct OpenWithOptions {
    pub path: PathBuf,
    pub content_type: String,
    pub applications: Vec<OpenWithApplication>,
}

fn default_application(options: &OpenWithOptions) -> Option<gio::AppInfo> {
    options
        .applications
        .iter()
        .find(|application| application.is_default)
        .map(|application| application.app_info.clone())
}

pub async fn discover_open_with(path: PathBuf) -> Result<OpenWithOptions, glib::Error> {
    let file = gio::File::for_path(&path);
    let info = file
        .query_info_future(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    let content_type = info.content_type().ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::NotSupported,
            "The selected file has no detectable content type",
        )
    })?;
    // Local file opening accepts both file-only (%f/%F) and URI (%u/%U)
    // handlers. Dispatch chooses the matching GIO API after lookup.
    let default = gio::AppInfo::default_for_type(&content_type, false);
    let mut applications = gio::AppInfo::recommended_for_type(&content_type);
    applications.extend(gio::AppInfo::all_for_type(&content_type));
    if let Some(default) = default.as_ref() {
        applications.insert(0, default.clone());
    }

    Ok(OpenWithOptions {
        path,
        content_type: content_type.to_string(),
        applications: normalize_applications(applications, default.as_ref()),
    })
}

pub fn launch_with(
    application: &gio::AppInfo,
    path: &Path,
    callback: impl FnOnce(Result<(), glib::Error>) + 'static,
) {
    match application_launch_kind(application) {
        Some(ApplicationLaunchKind::Files) => {
            let file = gio::File::for_path(path);
            callback(application.launch(&[file], None::<&gio::AppLaunchContext>));
        }
        Some(ApplicationLaunchKind::Uris) => {
            let uri = local_file_uri(path);
            application.launch_uris_async(
                &[uri.as_str()],
                None::<&gio::AppLaunchContext>,
                None::<&gio::Cancellable>,
                callback,
            );
        }
        None => callback(Err(glib::Error::new(
            gio::IOErrorEnum::NotSupported,
            "The selected application accepts neither local files nor URIs",
        ))),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ApplicationLaunchKind {
    Files,
    Uris,
}

fn application_launch_kind(application: &gio::AppInfo) -> Option<ApplicationLaunchKind> {
    if application.supports_files() {
        Some(ApplicationLaunchKind::Files)
    } else if application.supports_uris() {
        Some(ApplicationLaunchKind::Uris)
    } else {
        None
    }
}

const ASSOCIATION_QUEUE_CAPACITY: usize = 2;
const ASSOCIATION_RESULT_CAPACITY: usize = 4;
const ASSOCIATION_TEXT_CAPACITY: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssociationChange {
    SetDefault {
        content_type: String,
        application_id: String,
        application_name: String,
    },
    Reset {
        content_type: String,
    },
}

impl AssociationChange {
    fn validate(&self) -> Result<(), AssociationChangeError> {
        let valid_text = |value: &str| {
            !value.trim().is_empty()
                && value.len() <= ASSOCIATION_TEXT_CAPACITY
                && !value.as_bytes().contains(&0)
        };
        match self {
            Self::SetDefault {
                content_type,
                application_id,
                application_name,
            } if valid_text(content_type)
                && valid_text(application_id)
                && valid_text(application_name) =>
            {
                Ok(())
            }
            Self::Reset { content_type } if valid_text(content_type) => Ok(()),
            _ => Err(AssociationChangeError::InvalidRequest),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AssociationChangeError {
    #[error("association request is invalid or too large")]
    InvalidRequest,
    #[error("selected application is no longer installed")]
    MissingApplication,
    #[error("desktop association change failed: {0}")]
    Desktop(String),
    #[error("association worker queue is full")]
    QueueFull,
    #[error("association worker is unavailable")]
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssociationChangeResult {
    pub change: AssociationChange,
    pub result: Result<(), AssociationChangeError>,
}

pub struct AssociationWorker {
    sender: Option<SyncSender<AssociationChange>>,
    results: Receiver<AssociationChangeResult>,
    worker: Option<JoinHandle<()>>,
}

impl AssociationWorker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel(ASSOCIATION_QUEUE_CAPACITY);
        let (result_sender, results) = mpsc::sync_channel(ASSOCIATION_RESULT_CAPACITY);
        let worker = thread::Builder::new()
            .name("floe-associations".to_owned())
            .spawn(move || {
                while let Ok(change) = requests.recv() {
                    let result = apply_association_change(&change);
                    let _ = result_sender.send(AssociationChangeResult { change, result });
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            results,
            worker: Some(worker),
        })
    }

    pub fn try_change(&self, change: AssociationChange) -> Result<(), AssociationChangeError> {
        change.validate()?;
        let Some(sender) = self.sender.as_ref() else {
            return Err(AssociationChangeError::Disconnected);
        };
        sender.try_send(change).map_err(|error| match error {
            TrySendError::Full(_) => AssociationChangeError::QueueFull,
            TrySendError::Disconnected(_) => AssociationChangeError::Disconnected,
        })
    }

    pub fn try_result(&self) -> Option<AssociationChangeResult> {
        self.results.try_recv().ok()
    }
}

pub fn queue_default_for_type(
    worker: Option<&AssociationWorker>,
    application: &gio::AppInfo,
    application_name: &str,
    content_type: &str,
) -> Result<(), AssociationChangeError> {
    let application_id = application
        .id()
        .map(|id| id.to_string())
        .ok_or(AssociationChangeError::MissingApplication)?;
    worker
        .ok_or(AssociationChangeError::Disconnected)?
        .try_change(AssociationChange::SetDefault {
            content_type: content_type.to_owned(),
            application_id,
            application_name: application_name.to_owned(),
        })
}

impl Drop for AssociationWorker {
    fn drop(&mut self) {
        self.sender.take();
        self.worker.take();
    }
}

fn apply_association_change(change: &AssociationChange) -> Result<(), AssociationChangeError> {
    change.validate()?;
    match change {
        AssociationChange::SetDefault {
            content_type,
            application_id,
            ..
        } => {
            let application = gio::AppInfo::all()
                .into_iter()
                .find(|application| application.id().as_deref() == Some(application_id.as_str()))
                .ok_or(AssociationChangeError::MissingApplication)?;
            application
                .set_as_default_for_type(content_type)
                .map_err(|error| AssociationChangeError::Desktop(error.message().to_string()))
        }
        AssociationChange::Reset { content_type } => {
            gio::AppInfo::reset_type_associations(content_type);
            Ok(())
        }
    }
}

fn normalize_applications(
    applications: Vec<gio::AppInfo>,
    default: Option<&gio::AppInfo>,
) -> Vec<OpenWithApplication> {
    let applications = applications
        .into_iter()
        .filter(|application| {
            application.should_show() && application_launch_kind(application).is_some()
        })
        .map(|app_info| OpenWithApplication {
            is_default: default.is_some_and(|default| app_info.equal(default)),
            display_name: app_info.display_name().to_string(),
            app_info,
        })
        .collect::<Vec<_>>();
    sort_and_deduplicate_applications(applications)
}

fn sort_and_deduplicate_applications(
    mut applications: Vec<OpenWithApplication>,
) -> Vec<OpenWithApplication> {
    let mut seen = HashSet::new();
    applications.retain(|application| seen.insert(application_key(&application.app_info)));
    applications.sort_by(|left, right| {
        right.is_default.cmp(&left.is_default).then_with(|| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
        })
    });
    applications
}

fn application_key(application: &gio::AppInfo) -> String {
    application.id().map_or_else(
        || {
            format!(
                "{}\0{}",
                application.display_name(),
                application.executable().display()
            )
        },
        |id| id.to_string(),
    )
}

fn local_file_uri(path: &Path) -> glib::GString {
    gio::File::for_path(path).uri()
}

#[cfg(all(test, unix))]
mod tests {
    use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::PathBuf};

    use gtk::gio::{self, prelude::*};
    use tempfile::tempdir;

    use super::{
        ApplicationLaunchKind, OpenWithApplication, OpenWithOptions, application_launch_kind,
        default_application, local_file_uri, normalize_applications,
        sort_and_deduplicate_applications,
    };

    #[test]
    fn local_uri_round_trip_preserves_non_utf8_path() {
        let path = PathBuf::from("/tmp").join(OsString::from_vec(vec![b'f', b'o', 0x80]));
        let uri = local_file_uri(&path);
        let decoded = gio::File::for_uri(&uri).path();

        assert_eq!(decoded.as_deref(), Some(path.as_path()));
        assert!(!uri.contains('\u{fffd}'));
    }

    #[test]
    fn adversarial_file_only_launcher() {
        let root = tempdir().expect("temporary desktop fixtures");
        let file_only_path = root.path().join("file-only.desktop");
        let files_only_path = root.path().join("files-only.desktop");
        let uri_only_path = root.path().join("uri-only.desktop");
        fs::write(
            &file_only_path,
            "[Desktop Entry]\nType=Application\nName=File Only\nExec=/usr/bin/true %f\n",
        )
        .expect("file-only desktop fixture");
        fs::write(
            &files_only_path,
            "[Desktop Entry]\nType=Application\nName=Files Only\nExec=/usr/bin/true %F\n",
        )
        .expect("multi-file desktop fixture");
        fs::write(
            &uri_only_path,
            "[Desktop Entry]\nType=Application\nName=URI Handler\nExec=/usr/bin/true %u\n",
        )
        .expect("URI desktop fixture");
        let file_only = gio::DesktopAppInfo::from_filename(&file_only_path)
            .expect("file-only fixture app info")
            .upcast::<gio::AppInfo>();
        let files_only = gio::DesktopAppInfo::from_filename(&files_only_path)
            .expect("multi-file fixture app info")
            .upcast::<gio::AppInfo>();
        let uri_only = gio::DesktopAppInfo::from_filename(&uri_only_path)
            .expect("URI fixture app info")
            .upcast::<gio::AppInfo>();

        assert!(file_only.supports_files());
        assert!(!file_only.supports_uris());
        assert_eq!(
            application_launch_kind(&file_only),
            Some(ApplicationLaunchKind::Files)
        );
        assert_eq!(
            application_launch_kind(&files_only),
            Some(ApplicationLaunchKind::Files)
        );
        assert_eq!(
            application_launch_kind(&uri_only),
            Some(ApplicationLaunchKind::Uris)
        );

        let choices = normalize_applications(
            vec![file_only.clone(), files_only.clone(), uri_only.clone()],
            None,
        );
        assert!(
            choices
                .iter()
                .any(|choice| choice.app_info.equal(&file_only)),
            "visible file-only applications must remain in Open With"
        );
        assert!(
            choices
                .iter()
                .any(|choice| choice.app_info.equal(&files_only)),
            "visible multi-file-only applications must remain in Open With"
        );
        assert!(
            choices
                .iter()
                .any(|choice| choice.app_info.equal(&uri_only)),
            "URI handlers must remain in Open With"
        );
    }

    #[test]
    fn phase_5d_application_choices_are_deduplicated_and_default_first() {
        let alpha = gio::AppInfo::create_from_commandline(
            "/usr/bin/alpha",
            Some("Alpha"),
            gio::AppInfoCreateFlags::SUPPORTS_URIS,
        )
        .expect("fixture app info");
        let zulu = gio::AppInfo::create_from_commandline(
            "/usr/bin/zulu",
            Some("Zulu"),
            gio::AppInfoCreateFlags::SUPPORTS_URIS,
        )
        .expect("fixture app info");
        let choices = sort_and_deduplicate_applications(vec![
            OpenWithApplication {
                app_info: alpha.clone(),
                display_name: "Alpha".to_owned(),
                is_default: false,
            },
            OpenWithApplication {
                app_info: zulu,
                display_name: "Zulu".to_owned(),
                is_default: true,
            },
            OpenWithApplication {
                app_info: alpha,
                display_name: "Alpha".to_owned(),
                is_default: false,
            },
        ]);

        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].display_name, "Zulu");
        assert!(choices[0].is_default);
        assert_eq!(choices[1].display_name, "Alpha");
    }

    #[test]
    fn phase_6i_no_default_routes_to_chooser_options() {
        let application = gio::AppInfo::create_from_commandline(
            "/usr/bin/alpha",
            Some("Alpha"),
            gio::AppInfoCreateFlags::SUPPORTS_URIS,
        )
        .expect("fixture app info");
        let options = OpenWithOptions {
            path: PathBuf::from("/tmp/no-default.bin"),
            content_type: "application/octet-stream".into(),
            applications: vec![OpenWithApplication {
                app_info: application,
                display_name: "Alpha".into(),
                is_default: false,
            }],
        };

        assert!(default_application(&options).is_none());
        assert_eq!(options.path, PathBuf::from("/tmp/no-default.bin"));
    }

    #[test]
    fn phase_6i_registered_default_routes_to_that_application() {
        let application = gio::AppInfo::create_from_commandline(
            "/usr/bin/alpha",
            Some("Alpha"),
            gio::AppInfoCreateFlags::SUPPORTS_URIS,
        )
        .expect("fixture app info");
        let options = OpenWithOptions {
            path: PathBuf::from("/tmp/default.bin"),
            content_type: "application/octet-stream".into(),
            applications: vec![OpenWithApplication {
                app_info: application.clone(),
                display_name: "Alpha".into(),
                is_default: true,
            }],
        };

        let resolved = default_application(&options).expect("default should resolve");
        assert!(resolved.equal(&application));
    }
}
