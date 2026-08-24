use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use gtk::{
    gio::{self, prelude::*},
    glib,
};

pub enum DefaultLaunch {
    Launched,
    NoDefault(OpenWithOptions),
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
    let default = gio::AppInfo::default_for_type(&content_type, true);
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
    let uri = local_file_uri(path);
    application.launch_uris_async(
        &[uri.as_str()],
        None::<&gio::AppLaunchContext>,
        None::<&gio::Cancellable>,
        callback,
    );
}

pub fn set_default_for_type(
    application: &gio::AppInfo,
    content_type: &str,
) -> Result<(), glib::Error> {
    application.set_as_default_for_type(content_type)
}

fn normalize_applications(
    applications: Vec<gio::AppInfo>,
    default: Option<&gio::AppInfo>,
) -> Vec<OpenWithApplication> {
    let applications = applications
        .into_iter()
        .filter(|application| application.should_show() && application.supports_uris())
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
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use gtk::gio::{self, prelude::*};

    use super::{
        OpenWithApplication, OpenWithOptions, default_application, local_file_uri,
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
