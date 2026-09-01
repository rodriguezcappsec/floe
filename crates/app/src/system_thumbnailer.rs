use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use gtk::{gio, glib, prelude::*};
use rustix::fs::{Mode, OFlags};
use thiserror::Error;

const MAX_DEFINITION_BYTES: u64 = 256 * 1024;
pub(crate) const MAX_PROVIDER_OUTPUT_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const PROVIDER_TIMEOUT: Duration = Duration::from_secs(15);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct SystemThumbnailer {
    #[cfg(test)]
    definition_path: PathBuf,
    argv: Vec<OsString>,
    mime_types: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SystemThumbnailerRegistry {
    providers: Vec<SystemThumbnailer>,
    by_mime: HashMap<String, usize>,
}

#[derive(Debug, Error)]
pub(crate) enum SystemThumbnailerError {
    #[error("no reviewed system thumbnailer supports this content type")]
    Unsupported,
    #[error("thumbnailer definition is invalid: {0}")]
    InvalidDefinition(String),
    #[error("content type could not be resolved: {0}")]
    ContentType(String),
    #[error("thumbnailer process could not be started: {0}")]
    Spawn(#[source] io::Error),
    #[error("provider sandbox is unavailable")]
    SandboxUnavailable,
    #[error("thumbnailer process failed with status {0}")]
    ProcessFailed(i32),
    #[error("thumbnailer process exceeded its time limit")]
    TimedOut,
    #[error("thumbnailer request was cancelled")]
    Cancelled,
    #[error("thumbnailer output exceeds its safety limit")]
    OutputTooLarge,
    #[error("thumbnailer I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderOutput {
    pub(crate) bytes: Vec<u8>,
    pub(crate) content_type: String,
}

#[derive(Clone, Debug)]
struct ExecutionConfig {
    temporary_root: PathBuf,
    timeout: Duration,
    sandbox: SandboxMode,
}

#[derive(Clone, Debug)]
enum SandboxMode {
    Required(PathBuf),
    #[cfg(test)]
    DirectFixture,
}

impl ExecutionConfig {
    #[cfg(not(test))]
    fn production() -> Self {
        Self {
            temporary_root: std::env::temp_dir(),
            timeout: PROVIDER_TIMEOUT,
            sandbox: SandboxMode::Required(
                [PathBuf::from("/usr/bin/bwrap"), PathBuf::from("/bin/bwrap")]
                    .into_iter()
                    .find(|path| path.is_file())
                    .unwrap_or_default(),
            ),
        }
    }
}

impl SystemThumbnailerRegistry {
    pub(crate) fn discover() -> Self {
        let mut data_dirs = vec![glib::user_data_dir()];
        data_dirs.extend(glib::system_data_dirs());
        Self::discover_from_data_dirs(&data_dirs)
    }

    pub(crate) fn discover_from_data_dirs(data_dirs: &[PathBuf]) -> Self {
        let mut providers = Vec::new();
        let mut seen_definitions = HashSet::new();

        for data_dir in data_dirs {
            let directory = data_dir.join("thumbnailers");
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by_key(fs::DirEntry::file_name);

            for entry in entries {
                let name = entry.file_name();
                if Path::new(&name).extension() != Some(OsStr::new("thumbnailer"))
                    || !seen_definitions.insert(name)
                {
                    continue;
                }
                match parse_thumbnailer(&entry.path()) {
                    Ok(provider) => providers.push(provider),
                    Err(error) => {
                        tracing::debug!(
                            definition = %entry.path().display(),
                            %error,
                            "ignoring unavailable system thumbnailer"
                        );
                    }
                }
            }
        }

        let mut by_mime = HashMap::new();
        for (index, provider) in providers.iter().enumerate() {
            for mime_type in &provider.mime_types {
                by_mime.entry(mime_type.clone()).or_insert(index);
            }
        }

        Self { providers, by_mime }
    }

    pub(crate) fn generate(
        &self,
        source: &Path,
        requested_edge: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<ProviderOutput, SystemThumbnailerError> {
        #[cfg(not(test))]
        let config = ExecutionConfig::production();
        #[cfg(test)]
        let config = ExecutionConfig {
            temporary_root: std::env::temp_dir(),
            timeout: PROVIDER_TIMEOUT,
            sandbox: SandboxMode::DirectFixture,
        };
        self.generate_with_config(source, requested_edge, &config, cancelled)
    }

    pub(crate) fn supports_path(&self, source: &Path) -> Result<(), SystemThumbnailerError> {
        let content_type = content_type_for_path(source)?;
        if !phase_6l_mime_allowed(&content_type)
            || self.provider_for_content_type(&content_type).is_none()
        {
            return Err(SystemThumbnailerError::Unsupported);
        }
        Ok(())
    }

    fn generate_with_config(
        &self,
        source: &Path,
        requested_edge: u32,
        config: &ExecutionConfig,
        cancelled: impl Fn() -> bool,
    ) -> Result<ProviderOutput, SystemThumbnailerError> {
        let content_type = content_type_for_path(source)?;
        if !phase_6l_mime_allowed(&content_type) {
            return Err(SystemThumbnailerError::Unsupported);
        }
        let provider = self
            .provider_for_content_type(&content_type)
            .ok_or(SystemThumbnailerError::Unsupported)?;
        let bytes = provider.execute(source, requested_edge, config, cancelled)?;
        Ok(ProviderOutput {
            bytes,
            content_type,
        })
    }

    fn provider_for_content_type(&self, content_type: &str) -> Option<&SystemThumbnailer> {
        self.by_mime
            .get(content_type)
            .and_then(|index| self.providers.get(*index))
            .or_else(|| {
                self.providers.iter().find(|provider| {
                    provider
                        .mime_types
                        .iter()
                        .any(|mime_type| gio::content_type_equals(content_type, mime_type))
                })
            })
    }

    #[cfg(test)]
    fn provider_path_for(&self, content_type: &str) -> Option<&Path> {
        self.provider_for_content_type(content_type)
            .map(|provider| provider.definition_path.as_path())
    }
}

impl SystemThumbnailer {
    fn execute(
        &self,
        source: &Path,
        requested_edge: u32,
        config: &ExecutionConfig,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<u8>, SystemThumbnailerError> {
        if cancelled() {
            return Err(SystemThumbnailerError::Cancelled);
        }
        let temporary = TemporaryOutput::create(&config.temporary_root)?;
        let (provider_source, provider_output) = match &config.sandbox {
            SandboxMode::Required(bwrap) if bwrap.as_os_str().is_empty() => {
                return Err(SystemThumbnailerError::SandboxUnavailable);
            }
            SandboxMode::Required(_) => (
                Path::new("/run/floe/input"),
                Path::new("/run/floe/output/thumbnail.png"),
            ),
            #[cfg(test)]
            SandboxMode::DirectFixture => (source, temporary.output.as_path()),
        };
        let uri = gio::File::for_path(provider_source).uri();
        let argv = expand_argv(
            &self.argv,
            provider_source,
            uri.as_str(),
            provider_output,
            requested_edge,
        )?;
        let (program, arguments) = argv
            .split_first()
            .ok_or_else(|| SystemThumbnailerError::InvalidDefinition("empty Exec".to_owned()))?;
        let mut command = match &config.sandbox {
            SandboxMode::Required(bwrap) => {
                let mut command = Command::new(bwrap);
                command.args([
                    OsStr::new("--die-with-parent"),
                    OsStr::new("--new-session"),
                    OsStr::new("--unshare-all"),
                    OsStr::new("--clearenv"),
                    OsStr::new("--proc"),
                    OsStr::new("/proc"),
                    OsStr::new("--dev"),
                    OsStr::new("/dev"),
                    OsStr::new("--tmpfs"),
                    OsStr::new("/tmp"),
                    OsStr::new("--ro-bind"),
                    OsStr::new("/usr"),
                    OsStr::new("/usr"),
                    OsStr::new("--symlink"),
                    OsStr::new("usr/bin"),
                    OsStr::new("/bin"),
                    OsStr::new("--symlink"),
                    OsStr::new("usr/lib"),
                    OsStr::new("/lib"),
                    OsStr::new("--symlink"),
                    OsStr::new("usr/lib"),
                    OsStr::new("/lib64"),
                    OsStr::new("--dir"),
                    OsStr::new("/run"),
                    OsStr::new("--dir"),
                    OsStr::new("/run/floe"),
                    OsStr::new("--ro-bind"),
                    source.as_os_str(),
                    OsStr::new("/run/floe/input"),
                    OsStr::new("--bind"),
                    temporary.directory.as_os_str(),
                    OsStr::new("/run/floe/output"),
                    OsStr::new("--setenv"),
                    OsStr::new("PATH"),
                    OsStr::new("/usr/bin:/bin"),
                    OsStr::new("--chdir"),
                    OsStr::new("/tmp"),
                    OsStr::new("--"),
                ]);
                command.arg(program).args(arguments);
                command
            }
            #[cfg(test)]
            SandboxMode::DirectFixture => {
                let mut command = Command::new(program);
                command.args(arguments).current_dir(&temporary.directory);
                command
            }
        };
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(SystemThumbnailerError::Spawn)?;
        let started = Instant::now();

        loop {
            if cancelled() {
                terminate_child(&mut child);
                return Err(SystemThumbnailerError::Cancelled);
            }
            if started.elapsed() >= config.timeout {
                terminate_child(&mut child);
                return Err(SystemThumbnailerError::TimedOut);
            }
            match child.try_wait()? {
                Some(status) if status.success() => break,
                Some(status) => {
                    return Err(SystemThumbnailerError::ProcessFailed(
                        status.code().unwrap_or(-1),
                    ));
                }
                None => thread::sleep(PROCESS_POLL_INTERVAL),
            }
        }

        read_regular_file_bounded(&temporary.output, MAX_PROVIDER_OUTPUT_BYTES).map_err(|error| {
            if error.kind() == io::ErrorKind::FileTooLarge {
                SystemThumbnailerError::OutputTooLarge
            } else {
                SystemThumbnailerError::Io(error)
            }
        })
    }
}

fn parse_thumbnailer(path: &Path) -> Result<SystemThumbnailer, SystemThumbnailerError> {
    let bytes = read_regular_file_bounded(path, MAX_DEFINITION_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        SystemThumbnailerError::InvalidDefinition("definition is not UTF-8".to_owned())
    })?;
    let mut in_group = false;
    let mut exec = None;
    let mut try_exec = None;
    let mut mime_types = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line == "[Thumbnailer Entry]";
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Exec" => exec = Some(value.trim().to_owned()),
            "TryExec" => try_exec = Some(value.trim().to_owned()),
            "MimeType" => mime_types = Some(value.trim().to_owned()),
            _ => {}
        }
    }

    if try_exec
        .as_deref()
        .is_some_and(|program| program.is_empty() || glib::find_program_in_path(program).is_none())
    {
        return Err(SystemThumbnailerError::InvalidDefinition(
            "TryExec is unavailable".to_owned(),
        ));
    }
    let exec = exec
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SystemThumbnailerError::InvalidDefinition("missing Exec".to_owned()))?;
    let argv = glib::shell_parse_argv(&exec)
        .map_err(|error| SystemThumbnailerError::InvalidDefinition(error.to_string()))?;
    validate_argv(&argv)?;
    let mime_types = mime_types
        .ok_or_else(|| SystemThumbnailerError::InvalidDefinition("missing MimeType".to_owned()))?
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .filter(|mime_type| phase_6l_mime_allowed(mime_type))
        .collect::<Vec<_>>();
    if mime_types.is_empty() {
        return Err(SystemThumbnailerError::InvalidDefinition(
            "definition has no Phase 6L MIME types".to_owned(),
        ));
    }

    Ok(SystemThumbnailer {
        #[cfg(test)]
        definition_path: path.to_path_buf(),
        argv,
        mime_types,
    })
}

fn validate_argv(argv: &[OsString]) -> Result<(), SystemThumbnailerError> {
    if argv.is_empty() || argv[0].is_empty() {
        return Err(SystemThumbnailerError::InvalidDefinition(
            "empty Exec".to_owned(),
        ));
    }
    if argv[0].as_bytes().contains(&b'%') {
        return Err(SystemThumbnailerError::InvalidDefinition(
            "Exec program must be a fixed executable".to_owned(),
        ));
    }
    let mut has_input = false;
    let mut has_output = false;
    for argument in argv {
        let bytes = argument.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'%' {
                index += 1;
                continue;
            }
            let code = *bytes.get(index + 1).ok_or_else(|| {
                SystemThumbnailerError::InvalidDefinition("trailing % in Exec".to_owned())
            })?;
            match code {
                b'i' | b'u' => has_input = true,
                b'o' => has_output = true,
                b's' | b'%' => {}
                _ => {
                    return Err(SystemThumbnailerError::InvalidDefinition(format!(
                        "unsupported Exec field code %{code}",
                        code = char::from(code)
                    )));
                }
            }
            index += 2;
        }
    }
    if !has_input || !has_output {
        return Err(SystemThumbnailerError::InvalidDefinition(
            "Exec requires an input field and %o".to_owned(),
        ));
    }
    Ok(())
}

fn expand_argv(
    argv: &[OsString],
    input: &Path,
    uri: &str,
    output: &Path,
    size: u32,
) -> Result<Vec<OsString>, SystemThumbnailerError> {
    argv.iter()
        .map(|argument| expand_argument(argument, input, uri, output, size))
        .collect()
}

fn expand_argument(
    template: &OsStr,
    input: &Path,
    uri: &str,
    output: &Path,
    size: u32,
) -> Result<OsString, SystemThumbnailerError> {
    let bytes = template.as_bytes();
    let mut expanded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            expanded.push(bytes[index]);
            index += 1;
            continue;
        }
        let code = *bytes.get(index + 1).ok_or_else(|| {
            SystemThumbnailerError::InvalidDefinition("trailing % in Exec".to_owned())
        })?;
        match code {
            b'i' => expanded.extend_from_slice(input.as_os_str().as_bytes()),
            b'u' => expanded.extend_from_slice(uri.as_bytes()),
            b'o' => expanded.extend_from_slice(output.as_os_str().as_bytes()),
            b's' => expanded.extend_from_slice(size.to_string().as_bytes()),
            b'%' => expanded.push(b'%'),
            _ => {
                return Err(SystemThumbnailerError::InvalidDefinition(format!(
                    "unsupported Exec field code %{code}",
                    code = char::from(code)
                )));
            }
        }
        index += 2;
    }
    Ok(OsString::from_vec(expanded))
}

fn content_type_for_path(path: &Path) -> Result<String, SystemThumbnailerError> {
    let file = gio::File::for_path(path);
    let info = file
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| SystemThumbnailerError::ContentType(error.to_string()))?;
    info.content_type()
        .map(|content_type| content_type.to_string())
        .ok_or_else(|| SystemThumbnailerError::ContentType("content type is absent".to_owned()))
}

fn phase_6l_mime_allowed(mime_type: &str) -> bool {
    mime_type.starts_with("video/")
        || mime_type.starts_with("audio/")
        || mime_type.starts_with("font/")
        || mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/pdf"
                | "application/msword"
                | "application/vnd.ms-excel"
                | "application/vnd.ms-powerpoint"
                | "application/vnd.oasis.opendocument.chart"
                | "application/vnd.oasis.opendocument.formula"
                | "application/vnd.oasis.opendocument.graphics"
                | "application/vnd.oasis.opendocument.graphics-template"
                | "application/vnd.oasis.opendocument.image"
                | "application/vnd.oasis.opendocument.presentation"
                | "application/vnd.oasis.opendocument.presentation-template"
                | "application/vnd.oasis.opendocument.spreadsheet"
                | "application/vnd.oasis.opendocument.spreadsheet-template"
                | "application/vnd.oasis.opendocument.text"
                | "application/vnd.oasis.opendocument.text-master"
                | "application/vnd.oasis.opendocument.text-template"
                | "application/zip"
                | "application/x-7z-compressed"
                | "application/x-bzip2"
                | "application/x-gzip"
                | "application/x-rar"
                | "application/vnd.rar"
                | "application/x-tar"
                | "application/x-xz"
                | "application/zstd"
                | "application/java-archive"
        )
        || mime_type.starts_with("application/vnd.openxmlformats-officedocument.")
        || mime_type.starts_with("application/vnd.sun.xml.")
        || mime_type.starts_with("application/font-")
        || mime_type.starts_with("application/x-font-")
}

fn read_regular_file_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "path is not a regular file",
        ));
    }
    if metadata.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "file exceeds its safety limit",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref().take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "file exceeds its safety limit",
        ));
    }
    Ok(bytes)
}

fn terminate_child(child: &mut std::process::Child) {
    let group_killed = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .is_some_and(|process_group| {
            rustix::process::kill_process_group(process_group, rustix::process::Signal::KILL)
                .is_ok()
        });
    if !group_killed {
        let _ = child.kill();
    }
    let _ = child.wait();
}

struct TemporaryOutput {
    directory: PathBuf,
    output: PathBuf,
}

impl TemporaryOutput {
    fn create(root: &Path) -> io::Result<Self> {
        for _ in 0..128 {
            let id = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
            let directory = root.join(format!(".floe-thumbnail-{}-{id}", std::process::id()));
            match rustix::fs::mkdir(&directory, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                Ok(()) => {
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                    let output = directory.join("thumbnail.png");
                    return Ok(Self { directory, output });
                }
                Err(error) if error == rustix::io::Errno::EXIST => continue,
                Err(error) => return Err(io::Error::from(error)),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique thumbnail directory",
        ))
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.directory) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(%error, "could not remove system thumbnail temporary directory");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use tempfile::tempdir;

    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn write_definition(data_dir: &Path, name: &str, exec: &str, mime: &str) -> PathBuf {
        let directory = data_dir.join("thumbnailers");
        fs::create_dir_all(&directory).expect("thumbnailer directory should be created");
        let path = directory.join(name);
        fs::write(
            &path,
            format!("[Thumbnailer Entry]\nExec={exec}\nMimeType={mime};\n"),
        )
        .expect("thumbnailer definition should be written");
        path
    }

    fn write_script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n"))
            .expect("provider script should be written");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("provider script should be executable");
    }

    fn temporary_entries(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .expect("temporary root should be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.as_bytes().starts_with(b".floe-thumbnail-"))
            })
            .collect()
    }

    #[test]
    fn phase_6l_provider_precedence_is_user_first_and_filename_deterministic() {
        let root = tempdir().expect("temporary directory should be created");
        let user = root.path().join("user");
        let system = root.path().join("system");
        let user_shared = write_definition(
            &user,
            "shared.thumbnailer",
            "/bin/true %i %o",
            "text/plain; application/x-7z-compressed",
        );
        write_definition(
            &system,
            "shared.thumbnailer",
            "/bin/false %i %o",
            "text/plain",
        );
        write_definition(
            &user,
            "z-last.thumbnailer",
            "/bin/false %i %o",
            "application/pdf",
        );
        let first_pdf = write_definition(
            &user,
            "a-first.thumbnailer",
            "/bin/true %i %o",
            "application/pdf",
        );

        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[user, system]);
        assert_eq!(
            registry.provider_path_for("text/plain"),
            Some(user_shared.as_path())
        );
        assert_eq!(
            registry.provider_path_for("application/x-7z-compressed"),
            Some(user_shared.as_path())
        );
        assert_eq!(
            registry.provider_path_for("application/pdf"),
            Some(first_pdf.as_path())
        );
    }

    #[test]
    fn phase_6l_provider_rejects_unsafe_definitions_and_unreviewed_mime_types() {
        let root = tempdir().expect("temporary directory should be created");
        let data = root.path().join("data");
        write_definition(
            &data,
            "missing-output.thumbnailer",
            "/bin/true %i",
            "text/plain",
        );
        write_definition(
            &data,
            "unknown-code.thumbnailer",
            "/bin/true %i %o %x",
            "text/plain",
        );
        write_definition(&data, "dynamic-program.thumbnailer", "%i %o", "text/plain");
        write_definition(&data, "svg.thumbnailer", "/bin/true %i %o", "image/svg+xml");
        write_definition(
            &data,
            "executable.thumbnailer",
            "/bin/true %i %o",
            "application/x-executable",
        );
        let missing_try_exec = data.join("thumbnailers/missing-try-exec.thumbnailer");
        fs::write(
            &missing_try_exec,
            "[Thumbnailer Entry]\nTryExec=floe-provider-that-does-not-exist\nExec=/bin/true %i %o\nMimeType=text/plain;\n",
        )
        .expect("definition should be written");

        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
        assert!(registry.providers.is_empty());
        assert!(registry.provider_path_for("image/svg+xml").is_none());
        assert!(
            registry
                .provider_path_for("application/x-executable")
                .is_none()
        );
    }

    #[test]
    fn phase_6l_provider_expansion_preserves_raw_input_and_reviewed_field_codes() {
        let raw_path = PathBuf::from(OsString::from_vec(vec![b'f', 0x80, b'.', b'p', b'd', b'f']));
        let output = Path::new("/tmp/output.png");
        let expanded = expand_argv(
            &[OsString::from("prefix-%i"), OsString::from("%u|%o|%s|%%")],
            &raw_path,
            "file:///f%80.pdf",
            output,
            192,
        )
        .expect("reviewed codes should expand");
        assert_eq!(
            expanded[0].as_bytes(),
            [b"prefix-".as_slice(), raw_path.as_os_str().as_bytes()].concat()
        );
        assert_eq!(expanded[1], "file:///f%80.pdf|/tmp/output.png|192|%");
    }

    #[test]
    fn phase_6l_execution_success_uses_private_output_and_cleans_it() {
        let root = tempdir().expect("temporary directory should be created");
        let data = root.path().join("data");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root should be created");
        let fixture = root.path().join("fixture.png");
        fs::write(&fixture, PNG_1X1).expect("PNG fixture should be written");
        let script = root.path().join("provider");
        write_script(&script, &format!("cp '{}' \"$2\"", fixture.display()));
        write_definition(
            &data,
            "provider.thumbnailer",
            &format!("{} %i %o %s", script.display()),
            "text/plain",
        );
        let source = root.path().join("source.txt");
        fs::write(&source, b"passive text").expect("source should be written");
        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
        let output = registry
            .generate_with_config(
                &source,
                192,
                &ExecutionConfig {
                    temporary_root: temporary.clone(),
                    timeout: Duration::from_secs(2),
                    sandbox: SandboxMode::DirectFixture,
                },
                || false,
            )
            .expect("controlled provider should succeed");
        assert_eq!(output.bytes, PNG_1X1);
        assert_eq!(output.content_type, "text/plain");
        assert!(temporary_entries(&temporary).is_empty());
    }

    #[test]
    fn phase_6l_execution_reports_failure_timeout_and_cancellation_with_cleanup() {
        let root = tempdir().expect("temporary directory should be created");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root should be created");
        let source = root.path().join("source.txt");
        fs::write(&source, b"text").expect("source should be written");

        for (name, body, timeout, cancellation_after, expected) in [
            (
                "failure",
                "exit 7",
                Duration::from_secs(1),
                usize::MAX,
                "status 7",
            ),
            (
                "timeout",
                "sleep 10",
                Duration::from_millis(40),
                usize::MAX,
                "time limit",
            ),
            ("cancel", "sleep 10", Duration::from_secs(1), 2, "cancelled"),
        ] {
            let data = root.path().join(format!("data-{name}"));
            let script = root.path().join(format!("provider-{name}"));
            write_script(&script, body);
            write_definition(
                &data,
                "provider.thumbnailer",
                &format!("{} %i %o", script.display()),
                "text/plain",
            );
            let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
            let polls = AtomicUsize::new(0);
            let error = registry
                .generate_with_config(
                    &source,
                    32,
                    &ExecutionConfig {
                        temporary_root: temporary.clone(),
                        timeout,
                        sandbox: SandboxMode::DirectFixture,
                    },
                    || polls.fetch_add(1, Ordering::Relaxed) >= cancellation_after,
                )
                .expect_err("provider should fail safely");
            assert!(
                error.to_string().contains(expected),
                "unexpected error: {error}"
            );
            assert!(temporary_entries(&temporary).is_empty());
        }
    }

    #[test]
    fn phase_6l_execution_timeout_terminates_provider_process_group() {
        let root = tempdir().expect("temporary directory should be created");
        let data = root.path().join("data");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root should be created");
        let source = root.path().join("source.txt");
        fs::write(&source, b"text").expect("source should be written");
        let descendant_pid = root.path().join("descendant-pid");
        let script = root.path().join("provider");
        write_script(
            &script,
            &format!(
                "sleep 10 &\nprintf '%s' \"$!\" > '{}'\nwait",
                descendant_pid.display()
            ),
        );
        write_definition(
            &data,
            "provider.thumbnailer",
            &format!("{} %i %o", script.display()),
            "text/plain",
        );
        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
        assert!(matches!(
            registry.generate_with_config(
                &source,
                32,
                &ExecutionConfig {
                    temporary_root: temporary.clone(),
                    timeout: Duration::from_millis(60),
                    sandbox: SandboxMode::DirectFixture,
                },
                || false,
            ),
            Err(SystemThumbnailerError::TimedOut)
        ));
        let raw_pid = fs::read_to_string(&descendant_pid)
            .expect("provider should record descendant PID")
            .parse::<i32>()
            .expect("descendant PID should parse");
        let pid = rustix::process::Pid::from_raw(raw_pid).expect("PID should be positive");
        for _ in 0..100 {
            if rustix::process::test_kill_process(pid).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(rustix::process::test_kill_process(pid).is_err());
        assert!(temporary_entries(&temporary).is_empty());
    }

    #[test]
    fn phase_6l_execution_rejects_missing_symlink_and_oversized_output() {
        let root = tempdir().expect("temporary directory should be created");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root should be created");
        let source = root.path().join("source.txt");
        fs::write(&source, b"text").expect("source should be written");

        for (name, body) in [
            ("missing", ":"),
            ("symlink", "ln -s \"$1\" \"$2\""),
            ("oversized", "truncate -s 33554433 \"$2\""),
        ] {
            let data = root.path().join(format!("data-{name}"));
            let script = root.path().join(format!("provider-{name}"));
            write_script(&script, body);
            write_definition(
                &data,
                "provider.thumbnailer",
                &format!("{} %i %o", script.display()),
                "text/plain",
            );
            let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
            assert!(
                registry
                    .generate_with_config(
                        &source,
                        32,
                        &ExecutionConfig {
                            temporary_root: temporary.clone(),
                            timeout: Duration::from_secs(1),
                            sandbox: SandboxMode::DirectFixture,
                        },
                        || false,
                    )
                    .is_err()
            );
            assert!(temporary_entries(&temporary).is_empty());
        }

        let oversized = root.path().join("oversized.png");
        fs::File::create(&oversized)
            .expect("oversized fixture should be created")
            .set_len(MAX_PROVIDER_OUTPUT_BYTES + 1)
            .expect("oversized fixture should be sparse");
        let error = read_regular_file_bounded(&oversized, MAX_PROVIDER_OUTPUT_BYTES)
            .expect_err("oversized output should be rejected");
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    }

    #[test]
    fn phase_18l_policy_fails_closed_without_a_sandbox_launcher() {
        let root = tempdir().expect("temporary directory");
        let data = root.path().join("data");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root");
        let source = root.path().join("source.txt");
        fs::write(&source, PNG_1X1).expect("source fixture");
        write_definition(
            &data,
            "provider.thumbnailer",
            "/usr/bin/cp %i %o",
            "text/plain",
        );
        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
        let error = registry
            .generate_with_config(
                &source,
                32,
                &ExecutionConfig {
                    temporary_root: temporary,
                    timeout: Duration::from_secs(1),
                    sandbox: SandboxMode::Required(PathBuf::new()),
                },
                || false,
            )
            .expect_err("missing sandbox must not run provider directly");
        assert!(matches!(error, SystemThumbnailerError::SandboxUnavailable));
    }

    #[test]
    fn phase_18l_presentation_keeps_provider_failure_states_distinct() {
        let unavailable = SystemThumbnailerError::SandboxUnavailable.to_string();
        let timed_out = SystemThumbnailerError::TimedOut.to_string();
        let unsupported = SystemThumbnailerError::Unsupported.to_string();
        let cancelled = SystemThumbnailerError::Cancelled.to_string();

        assert!(unavailable.contains("sandbox"));
        assert!(unavailable.contains("unavailable"));
        assert!(timed_out.contains("time limit"));
        assert!(unsupported.contains("no reviewed system thumbnailer"));
        assert!(cancelled.contains("cancelled"));
        assert_ne!(unavailable, timed_out);
        assert_ne!(unavailable, unsupported);
        assert!(!unavailable.contains("safe"));
    }

    #[test]
    fn phase_18l_execution_uses_real_bubblewrap_when_available() {
        let bwrap = PathBuf::from("/usr/bin/bwrap");
        if !bwrap.is_file() {
            return;
        }
        let usable = Command::new(&bwrap)
            .args([
                "--unshare-all",
                "--ro-bind",
                "/usr",
                "/usr",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--",
                "/usr/bin/true",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !usable {
            return;
        }
        let root = tempdir().expect("temporary directory");
        let data = root.path().join("data");
        let temporary = root.path().join("temporary");
        fs::create_dir(&temporary).expect("temporary root");
        let source = root.path().join("source.txt");
        fs::write(&source, PNG_1X1).expect("source fixture");
        write_definition(
            &data,
            "provider.thumbnailer",
            "/usr/bin/cp %i %o",
            "text/plain",
        );
        let registry = SystemThumbnailerRegistry::discover_from_data_dirs(&[data]);
        let output = registry
            .generate_with_config(
                &source,
                32,
                &ExecutionConfig {
                    temporary_root: temporary.clone(),
                    timeout: Duration::from_secs(3),
                    sandbox: SandboxMode::Required(bwrap),
                },
                || false,
            )
            .expect("Bubblewrap provider");
        assert_eq!(output.bytes, PNG_1X1);
        assert!(temporary_entries(&temporary).is_empty());
    }
}
