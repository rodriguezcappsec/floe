use std::path::Path;

use gtk::{
    gio::{self, prelude::*},
    glib,
};

/// Opens a local path with the desktop's default application without losing
/// non-UTF-8 path bytes through an intermediate display string.
pub fn launch_default(path: &Path, callback: impl FnOnce(Result<(), glib::Error>) + 'static) {
    let uri = local_file_uri(path);
    gio::AppInfo::launch_default_for_uri_async(
        &uri,
        None::<&gio::AppLaunchContext>,
        None::<&gio::Cancellable>,
        callback,
    );
}

fn local_file_uri(path: &Path) -> glib::GString {
    gio::File::for_path(path).uri()
}

#[cfg(all(test, unix))]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use gtk::gio::{self, prelude::*};

    use super::local_file_uri;

    #[test]
    fn local_uri_round_trip_preserves_non_utf8_path() {
        let path = PathBuf::from("/tmp").join(OsString::from_vec(vec![b'f', b'o', 0x80]));
        let uri = local_file_uri(&path);
        let decoded = gio::File::for_uri(&uri).path();

        assert_eq!(decoded.as_deref(), Some(path.as_path()));
        assert!(!uri.contains('\u{fffd}'));
    }
}
