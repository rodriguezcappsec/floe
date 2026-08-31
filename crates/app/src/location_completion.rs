//! Bounded, superseding location-entry completion outside the GTK main loop.

use std::{
    ffi::OsStr,
    fs, io,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

pub const COMPLETION_RESULT_CAPACITY: usize = 64;
pub const COMPLETION_SCAN_CAPACITY: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionCandidate {
    pub path: PathBuf,
    pub display: String,
}

#[derive(Debug)]
pub struct CompletionResult {
    pub generation: u64,
    pub candidates: Vec<CompletionCandidate>,
    pub truncated: bool,
    pub error: Option<io::ErrorKind>,
}

#[derive(Debug)]
struct CompletionRequest {
    generation: u64,
    input: String,
}

#[derive(Default)]
struct Shared {
    request: Option<CompletionRequest>,
    result: Option<CompletionResult>,
    shutdown: bool,
}

pub struct LocationCompletionWorker {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    join: Option<JoinHandle<()>>,
}

impl LocationCompletionWorker {
    pub fn spawn() -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        let join = thread::Builder::new()
            .name("floe-location-completion".to_owned())
            .spawn(move || worker_loop(&worker_shared))?;
        Ok(Self {
            shared,
            join: Some(join),
        })
    }

    pub fn request(&self, generation: u64, input: String) {
        let (lock, wake) = &*self.shared;
        let mut shared = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        shared.request = Some(CompletionRequest { generation, input });
        wake.notify_one();
    }

    pub fn try_result(&self) -> Option<CompletionResult> {
        let (lock, _) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .result
            .take()
    }
}

impl Drop for LocationCompletionWorker {
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
        let request = {
            let (lock, wake) = &**shared;
            let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
            while state.request.is_none() && !state.shutdown {
                state = wake
                    .wait(state)
                    .unwrap_or_else(|poison| poison.into_inner());
            }
            if state.shutdown {
                return;
            }
            state.request.take().expect("request checked")
        };
        let result = complete(request);
        let (lock, _) = &**shared;
        let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        if state
            .request
            .as_ref()
            .is_none_or(|newer| newer.generation <= result.generation)
        {
            state.result = Some(result);
        }
    }
}

fn complete(request: CompletionRequest) -> CompletionResult {
    let Some((parent, prefix)) = completion_parent_and_prefix(&request.input) else {
        return CompletionResult {
            generation: request.generation,
            candidates: Vec::new(),
            truncated: false,
            error: None,
        };
    };
    let entries = match fs::read_dir(&parent) {
        Ok(entries) => entries,
        Err(error) => {
            return CompletionResult {
                generation: request.generation,
                candidates: Vec::new(),
                truncated: false,
                error: Some(error.kind()),
            };
        }
    };
    let mut candidates = Vec::new();
    let mut truncated = false;
    for (scanned, entry) in entries.enumerate() {
        if scanned == COMPLETION_SCAN_CAPACITY {
            truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        if !os_starts_with(&name, &prefix) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        candidates.push(CompletionCandidate {
            display: format!("{}/", path.to_string_lossy()),
            path,
        });
        if candidates.len() == COMPLETION_RESULT_CAPACITY {
            truncated = true;
            break;
        }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    CompletionResult {
        generation: request.generation,
        candidates,
        truncated,
        error: None,
    }
}

fn completion_parent_and_prefix(input: &str) -> Option<(PathBuf, std::ffi::OsString)> {
    if input.is_empty() {
        return None;
    }
    let path = PathBuf::from(input);
    if !path.is_absolute() {
        return None;
    }
    if input.ends_with('/') {
        return Some((path, std::ffi::OsString::new()));
    }
    Some((
        path.parent()?.to_path_buf(),
        path.file_name()?.to_os_string(),
    ))
}

#[cfg(unix)]
fn os_starts_with(value: &OsStr, prefix: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().starts_with(prefix.as_bytes())
}

#[cfg(not(unix))]
fn os_starts_with(value: &OsStr, prefix: &OsStr) -> bool {
    value
        .to_string_lossy()
        .starts_with(prefix.to_string_lossy().as_ref())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use tempfile::tempdir;

    use super::*;

    fn wait_result(worker: &LocationCompletionWorker) -> CompletionResult {
        for _ in 0..100 {
            if let Some(result) = worker.try_result() {
                return result;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("completion result timed out")
    }

    #[test]
    fn phase_7g_completion_finds_only_matching_directories() {
        let fixture = tempdir().expect("fixture");
        fs::create_dir(fixture.path().join("alpha")).expect("alpha");
        fs::create_dir(fixture.path().join("alpine")).expect("alpine");
        fs::create_dir(fixture.path().join("beta")).expect("beta");
        fs::write(fixture.path().join("also-file"), b"data").expect("file");
        let worker = LocationCompletionWorker::spawn().expect("worker");
        worker.request(7, format!("{}/al", fixture.path().display()));
        let result = wait_result(&worker);
        assert_eq!(result.generation, 7);
        assert_eq!(result.candidates.len(), 2);
        assert!(result.candidates.iter().all(|item| item.path.is_absolute()));
        assert!(!result.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn phase_7g_completion_preserves_non_utf8_candidate_identity() {
        let fixture = tempdir().expect("fixture");
        let raw = OsString::from_vec(b"raw-\xff".to_vec());
        let exact = fixture.path().join(&raw);
        fs::create_dir(&exact).expect("raw directory");
        let worker = LocationCompletionWorker::spawn().expect("worker");
        worker.request(9, format!("{}/raw-", fixture.path().display()));
        let result = wait_result(&worker);
        assert_eq!(result.candidates[0].path, exact);
        assert!(result.candidates[0].display.contains('\u{fffd}'));
    }

    #[test]
    fn phase_7g_completion_supersedes_pending_requests_and_bounds_results() {
        let fixture = tempdir().expect("fixture");
        for index in 0..(COMPLETION_RESULT_CAPACITY + 20) {
            fs::create_dir(fixture.path().join(format!("entry-{index:03}"))).expect("entry");
        }
        let worker = LocationCompletionWorker::spawn().expect("worker");
        worker.request(1, "/definitely/missing".to_owned());
        worker.request(2, format!("{}/entry-", fixture.path().display()));
        let result = wait_result(&worker);
        assert_eq!(result.generation, 2);
        assert_eq!(result.candidates.len(), COMPLETION_RESULT_CAPACITY);
        assert!(result.truncated);
    }
}
