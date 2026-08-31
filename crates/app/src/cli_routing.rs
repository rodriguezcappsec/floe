//! Bounded local command-line target classification for GApplication routing.

use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

pub const CLI_TARGET_PATH_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliRoute {
    Folder(PathBuf),
    Reveal(PathBuf),
}

#[derive(Debug)]
pub struct CliRouteResult {
    pub route: Result<CliRoute, CliRouteError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliRouteError {
    Relative,
    Oversized,
    Missing,
    Inaccessible,
    Unsupported,
}

#[derive(Default)]
struct Shared {
    pending: Option<PathBuf>,
    result: Option<CliRouteResult>,
    shutdown: bool,
}

pub struct CliRouteWorker {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    join: Option<JoinHandle<()>>,
}

impl CliRouteWorker {
    pub fn spawn() -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        let join = thread::Builder::new()
            .name("floe-cli-route".to_owned())
            .spawn(move || worker_loop(&worker_shared))?;
        Ok(Self {
            shared,
            join: Some(join),
        })
    }

    pub fn request(&self, path: PathBuf) {
        let (lock, wake) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pending = Some(path);
        wake.notify_one();
    }

    pub fn try_result(&self) -> Option<CliRouteResult> {
        let (lock, _) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .result
            .take()
    }
}

impl Drop for CliRouteWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .shutdown = true;
        wake.notify_one();
        self.join.take();
    }
}

fn worker_loop(shared: &Arc<(Mutex<Shared>, Condvar)>) {
    loop {
        let path = {
            let (lock, wake) = &**shared;
            let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
            while state.pending.is_none() && !state.shutdown {
                state = wake
                    .wait(state)
                    .unwrap_or_else(|poison| poison.into_inner());
            }
            if state.shutdown {
                return;
            }
            state.pending.take().expect("pending checked")
        };
        let result = CliRouteResult {
            route: classify_cli_target(path),
        };
        let (lock, _) = &**shared;
        let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.pending.is_none() {
            state.result = Some(result);
        }
    }
}

pub fn classify_cli_target(path: PathBuf) -> Result<CliRoute, CliRouteError> {
    if !path.is_absolute() {
        return Err(CliRouteError::Relative);
    }
    if path.as_os_str().as_encoded_bytes().len() > CLI_TARGET_PATH_BYTES {
        return Err(CliRouteError::Oversized);
    }
    let metadata = fs::metadata(&path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => CliRouteError::Missing,
        io::ErrorKind::PermissionDenied => CliRouteError::Inaccessible,
        _ => CliRouteError::Unsupported,
    })?;
    if metadata.is_dir() {
        Ok(CliRoute::Folder(path))
    } else if metadata.is_file() {
        Ok(CliRoute::Reveal(path))
    } else {
        Err(CliRouteError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_7g_cli_routes_folders_and_files_without_losing_identity() {
        let fixture = tempdir().expect("fixture");
        let file = fixture.path().join("file.txt");
        fs::write(&file, b"data").expect("file");
        assert_eq!(
            classify_cli_target(fixture.path().to_path_buf()),
            Ok(CliRoute::Folder(fixture.path().to_path_buf()))
        );
        assert_eq!(
            classify_cli_target(file.clone()),
            Ok(CliRoute::Reveal(file))
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_7g_cli_preserves_non_utf8_file_target() {
        let fixture = tempdir().expect("fixture");
        let file = fixture
            .path()
            .join(OsString::from_vec(b"file-\xff".to_vec()));
        fs::write(&file, b"data").expect("raw file");
        assert_eq!(
            classify_cli_target(file.clone()),
            Ok(CliRoute::Reveal(file))
        );
    }

    #[test]
    fn phase_7g_cli_worker_supersedes_and_reports_missing_targets() {
        let fixture = tempdir().expect("fixture");
        let worker = CliRouteWorker::spawn().expect("worker");
        worker.request(fixture.path().join("missing-one"));
        worker.request(fixture.path().join("missing-two"));
        for _ in 0..100 {
            if let Some(result) = worker.try_result() {
                assert_eq!(result.route, Err(CliRouteError::Missing));
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("CLI route result timed out")
    }
}
