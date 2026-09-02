//! Bounded, no-follow Unix permission evidence and conservative repair plans.

use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Read,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

use rustix::{
    fs::{IFlags, Mode, OFlags},
    io::Errno,
};
use thiserror::Error;

pub const PERMISSION_AUDIT_TARGET_CAPACITY: usize = 128;
const XATTR_NAME_BYTES_CAPACITY: usize = 64 * 1024;
const MOUNTINFO_BYTES_CAPACITY: u64 = 1024 * 1024;
const MOUNTINFO_ENTRY_CAPACITY: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionAuditRequest {
    targets: Vec<PathBuf>,
}

impl PermissionAuditRequest {
    pub fn new(targets: Vec<PathBuf>) -> Result<Self, PermissionAuditError> {
        if targets.is_empty() || targets.len() > PERMISSION_AUDIT_TARGET_CAPACITY {
            return Err(PermissionAuditError::InvalidTargetCount);
        }
        let mut unique = HashSet::with_capacity(targets.len());
        for target in &targets {
            if !target.is_absolute() {
                return Err(PermissionAuditError::RelativePath(target.clone()));
            }
            if !unique.insert(target.clone()) {
                return Err(PermissionAuditError::DuplicatePath(target.clone()));
            }
        }
        Ok(Self { targets })
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PermissionAuditError {
    #[error("select between 1 and {PERMISSION_AUDIT_TARGET_CAPACITY} permission-audit targets")]
    InvalidTargetCount,
    #[error("permission-audit target must be absolute: {}", .0.display())]
    RelativePath(PathBuf),
    #[error("permission-audit target is duplicated: {}", .0.display())]
    DuplicatePath(PathBuf),
    #[error("permission audit cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PermissionAuditReport {
    pub entries: Vec<PermissionAuditEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionAuditEntry {
    pub path: PathBuf,
    pub state: PermissionAuditEntryState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionAuditEntryState {
    Inspected(Box<PermissionEvidence>),
    SymbolicLink,
    Changed,
    Inaccessible(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionObjectKind {
    RegularFile,
    Directory,
    Other,
}

impl PermissionObjectKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RegularFile => "regular file",
            Self::Directory => "directory",
            Self::Other => "special filesystem object",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionProbe<T> {
    Known(T),
    Unsupported,
    Limited(String),
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XattrSummary {
    pub total_names: usize,
    pub user_names: usize,
    pub security_names: usize,
    pub access_acl: bool,
    pub default_acl: bool,
    pub linux_capability: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountContext {
    pub mount_point: PathBuf,
    pub filesystem_type: OsString,
    pub read_only: bool,
    pub no_exec: bool,
    pub no_suid: bool,
    pub no_dev: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionEvidence {
    pub identity: PermissionAuditIdentity,
    pub object_kind: PermissionObjectKind,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub xattrs: PermissionProbe<XattrSummary>,
    pub immutable: PermissionProbe<bool>,
    pub mount: PermissionProbe<MountContext>,
    pub findings: Vec<PermissionFinding>,
    pub conservative_fix: Option<PermissionModeFix>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionFindingSeverity {
    Information,
    Review,
    High,
}

impl PermissionFindingSeverity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Information => "Information",
            Self::Review => "Review",
            Self::High => "High attention",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PermissionFindingKind {
    WorldWritable,
    GroupWritable,
    SensitiveNameBroadAccess,
    SetUserId,
    SetGroupId,
    StickyRegularFile,
    ForeignOwner,
    AccessControlList,
    LinuxCapability,
    Immutable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionFinding {
    pub kind: PermissionFindingKind,
    pub severity: PermissionFindingSeverity,
    pub title: &'static str,
    pub explanation: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionModeFix {
    pub object_kind: PermissionObjectKind,
    pub original_mode: u32,
    pub proposed_mode: u32,
    pub reasons: Vec<PermissionFindingKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionAuditIdentity {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub modified_seconds: i64,
    pub modified_nanoseconds: i64,
    pub changed_seconds: i64,
    pub changed_nanoseconds: i64,
}

impl PermissionAuditIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

pub fn audit_permissions(
    request: &PermissionAuditRequest,
    cancelled: impl Fn() -> bool,
) -> Result<PermissionAuditReport, PermissionAuditError> {
    let mount_table = read_mount_table();
    let current_uid = rustix::process::getuid().as_raw();
    let mut entries = Vec::with_capacity(request.targets.len());
    for path in request.targets() {
        if cancelled() {
            return Err(PermissionAuditError::Cancelled);
        }
        entries.push(audit_one(path, current_uid, &mount_table));
    }
    Ok(PermissionAuditReport { entries })
}

fn audit_one(
    path: &Path,
    current_uid: u32,
    mount_table: &Result<Vec<MountContext>, String>,
) -> PermissionAuditEntry {
    audit_one_with_hook(path, current_uid, mount_table, || {})
}

fn audit_one_with_hook(
    path: &Path,
    current_uid: u32,
    mount_table: &Result<Vec<MountContext>, String>,
    after_snapshot: impl FnOnce(),
) -> PermissionAuditEntry {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return PermissionAuditEntry {
                path: path.to_path_buf(),
                state: PermissionAuditEntryState::Inaccessible(error.to_string()),
            };
        }
    };
    if before.file_type().is_symlink() {
        return PermissionAuditEntry {
            path: path.to_path_buf(),
            state: PermissionAuditEntryState::SymbolicLink,
        };
    }

    let before_identity = PermissionAuditIdentity::from_metadata(&before);
    let object_kind = if before.is_file() {
        PermissionObjectKind::RegularFile
    } else if before.is_dir() {
        PermissionObjectKind::Directory
    } else {
        PermissionObjectKind::Other
    };
    after_snapshot();

    let xattrs = query_xattrs(path);
    let immutable = query_immutable(path, object_kind);
    let mount = match mount_table {
        Ok(table) => find_mount_context(path, table).cloned().map_or_else(
            || PermissionProbe::Unavailable("no enclosing mount was found".to_owned()),
            PermissionProbe::Known,
        ),
        Err(error) => PermissionProbe::Unavailable(error.clone()),
    };

    let after = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return PermissionAuditEntry {
                path: path.to_path_buf(),
                state: PermissionAuditEntryState::Changed,
            };
        }
    };
    if after.file_type().is_symlink()
        || PermissionAuditIdentity::from_metadata(&after) != before_identity
    {
        return PermissionAuditEntry {
            path: path.to_path_buf(),
            state: PermissionAuditEntryState::Changed,
        };
    }

    let mut evidence = PermissionEvidence {
        identity: before_identity,
        object_kind,
        mode: before.mode() & 0o7777,
        uid: before.uid(),
        gid: before.gid(),
        xattrs,
        immutable,
        mount,
        findings: Vec::new(),
        conservative_fix: None,
    };
    evidence.findings = permission_findings(path, &evidence, current_uid);
    evidence.conservative_fix = conservative_mode_fix(&evidence);
    PermissionAuditEntry {
        path: path.to_path_buf(),
        state: PermissionAuditEntryState::Inspected(Box::new(evidence)),
    }
}

pub fn symbolic_mode(mode: u32) -> String {
    let mut output = String::with_capacity(9);
    let bits = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (index, (bit, ordinary)) in bits.into_iter().enumerate() {
        let enabled = mode & bit != 0;
        let rendered = match index {
            2 if mode & 0o4000 != 0 => {
                if enabled {
                    's'
                } else {
                    'S'
                }
            }
            5 if mode & 0o2000 != 0 => {
                if enabled {
                    's'
                } else {
                    'S'
                }
            }
            8 if mode & 0o1000 != 0 => {
                if enabled {
                    't'
                } else {
                    'T'
                }
            }
            _ if enabled => ordinary,
            _ => '-',
        };
        output.push(rendered);
    }
    output
}

pub fn permission_findings(
    path: &Path,
    evidence: &PermissionEvidence,
    current_uid: u32,
) -> Vec<PermissionFinding> {
    let mut findings = Vec::new();
    let mode = evidence.mode;
    if mode & 0o002 != 0 {
        findings.push(finding(
            PermissionFindingKind::WorldWritable,
            PermissionFindingSeverity::High,
            "Writable by every local user",
            "The Unix other-write bit lets any local account permitted by the mount modify this item.",
        ));
    }
    if mode & 0o020 != 0 {
        findings.push(finding(
            PermissionFindingKind::GroupWritable,
            PermissionFindingSeverity::Review,
            "Writable by the owning group",
            "Members of the owning group may modify this item; that can be intentional for shared work.",
        ));
    }
    if evidence.object_kind == PermissionObjectKind::RegularFile
        && sensitive_filename(path.file_name().unwrap_or(path.as_os_str()))
        && mode & 0o077 != 0
    {
        findings.push(finding(
            PermissionFindingKind::SensitiveNameBroadAccess,
            PermissionFindingSeverity::High,
            "Sensitive-looking file has group or other access",
            "The filename resembles a private key or credential file. This is a filename heuristic; Floe did not read its contents.",
        ));
    }
    if mode & 0o4000 != 0 {
        findings.push(finding(
            PermissionFindingKind::SetUserId,
            PermissionFindingSeverity::High,
            "Set-user-ID bit is enabled",
            "Executing this file may use the file owner's identity where the filesystem and kernel permit it.",
        ));
    }
    if mode & 0o2000 != 0 {
        findings.push(finding(
            PermissionFindingKind::SetGroupId,
            PermissionFindingSeverity::Review,
            "Set-group-ID bit is enabled",
            "Execution or directory inheritance may use the owning group where the filesystem permits it.",
        ));
    }
    if evidence.object_kind == PermissionObjectKind::RegularFile && mode & 0o1000 != 0 {
        findings.push(finding(
            PermissionFindingKind::StickyRegularFile,
            PermissionFindingSeverity::Information,
            "Sticky bit is set on a regular file",
            "Linux normally gives the sticky bit useful meaning on directories, so this unusual mode deserves review.",
        ));
    }
    if evidence.uid != current_uid {
        findings.push(finding(
            PermissionFindingKind::ForeignOwner,
            PermissionFindingSeverity::Information,
            "Owned by another numeric user ID",
            "The selected item's owner differs from Floe's current user. Floe does not infer account trust from ownership.",
        ));
    }
    if let PermissionProbe::Known(xattrs) = &evidence.xattrs {
        if xattrs.access_acl || xattrs.default_acl {
            findings.push(finding(
                PermissionFindingKind::AccessControlList,
                PermissionFindingSeverity::Review,
                "POSIX access-control list is present",
                "An ACL can grant or restrict access beyond the displayed Unix mode bits; Floe does not edit it here.",
            ));
        }
        if xattrs.linux_capability {
            findings.push(finding(
                PermissionFindingKind::LinuxCapability,
                PermissionFindingSeverity::High,
                "Linux file capabilities are present",
                "The security.capability attribute may grant privileges when this file executes; Floe does not edit it here.",
            ));
        }
    }
    if matches!(evidence.immutable, PermissionProbe::Known(true)) {
        findings.push(finding(
            PermissionFindingKind::Immutable,
            PermissionFindingSeverity::Review,
            "Immutable inode flag is enabled",
            "The filesystem may reject changes until an authorized tool clears the immutable flag; Floe does not clear it.",
        ));
    }
    findings
}

fn finding(
    kind: PermissionFindingKind,
    severity: PermissionFindingSeverity,
    title: &'static str,
    explanation: &'static str,
) -> PermissionFinding {
    PermissionFinding {
        kind,
        severity,
        title,
        explanation,
    }
}

fn conservative_mode_fix(evidence: &PermissionEvidence) -> Option<PermissionModeFix> {
    let mut clear_bits = 0u32;
    let mut reasons = Vec::new();
    for finding in &evidence.findings {
        match finding.kind {
            PermissionFindingKind::WorldWritable => {
                clear_bits |= 0o002;
                reasons.push(finding.kind);
            }
            PermissionFindingKind::SensitiveNameBroadAccess => {
                clear_bits |= 0o077;
                reasons.push(finding.kind);
            }
            _ => {}
        }
    }
    let proposed_mode = evidence.mode & !clear_bits;
    (proposed_mode != evidence.mode).then_some(PermissionModeFix {
        object_kind: evidence.object_kind,
        original_mode: evidence.mode,
        proposed_mode,
        reasons,
    })
}

fn sensitive_filename(name: &OsStr) -> bool {
    let mut bytes = name.as_bytes().to_vec();
    bytes.make_ascii_lowercase();
    matches!(
        bytes.as_slice(),
        b"id_rsa"
            | b"id_dsa"
            | b"id_ecdsa"
            | b"id_ed25519"
            | b".netrc"
            | b"credentials"
            | b"credentials.json"
    ) || bytes == b".env"
        || bytes.starts_with(b".env.")
        || bytes.ends_with(b".pem")
        || bytes.ends_with(b".key")
        || bytes.ends_with(b".p12")
        || bytes.ends_with(b".pfx")
}

fn query_xattrs(path: &Path) -> PermissionProbe<XattrSummary> {
    let mut buffer = vec![0u8; XATTR_NAME_BYTES_CAPACITY];
    match rustix::fs::llistxattr(path, &mut buffer[..]) {
        Ok(length) => {
            let mut summary = XattrSummary {
                total_names: 0,
                user_names: 0,
                security_names: 0,
                access_acl: false,
                default_acl: false,
                linux_capability: false,
            };
            for name in buffer[..length].split(|byte| *byte == 0) {
                if name.is_empty() {
                    continue;
                }
                summary.total_names += 1;
                summary.user_names += usize::from(name.starts_with(b"user."));
                summary.security_names += usize::from(name.starts_with(b"security."));
                summary.access_acl |= name == b"system.posix_acl_access";
                summary.default_acl |= name == b"system.posix_acl_default";
                summary.linux_capability |= name == b"security.capability";
            }
            PermissionProbe::Known(summary)
        }
        Err(Errno::RANGE) => PermissionProbe::Limited(format!(
            "extended-attribute names exceed the {XATTR_NAME_BYTES_CAPACITY}-byte inspection bound"
        )),
        Err(error) => probe_error(error),
    }
}

fn query_immutable(path: &Path, object_kind: PermissionObjectKind) -> PermissionProbe<bool> {
    if object_kind == PermissionObjectKind::Other {
        return PermissionProbe::Unsupported;
    }
    let descriptor = match rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(error) => return probe_error(error),
    };
    match rustix::fs::ioctl_getflags(&descriptor) {
        Ok(flags) => PermissionProbe::Known(flags.contains(IFlags::IMMUTABLE)),
        Err(error) => probe_error(error),
    }
}

fn probe_error<T>(error: Errno) -> PermissionProbe<T> {
    if matches!(error, Errno::NOTSUP | Errno::NOTTY) {
        PermissionProbe::Unsupported
    } else if matches!(error, Errno::ACCESS | Errno::PERM) {
        PermissionProbe::Unavailable("permission denied while querying metadata".to_owned())
    } else {
        PermissionProbe::Unavailable(error.to_string())
    }
}

fn read_mount_table() -> Result<Vec<MountContext>, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32);
    let mut file = options
        .open("/proc/self/mountinfo")
        .map_err(|error| format!("mount context unavailable: {error}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MOUNTINFO_BYTES_CAPACITY + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("mount context unavailable: {error}"))?;
    if bytes.len() as u64 > MOUNTINFO_BYTES_CAPACITY {
        return Err(format!(
            "mount table exceeds the {MOUNTINFO_BYTES_CAPACITY}-byte inspection bound"
        ));
    }
    parse_mount_table(&bytes)
}

fn parse_mount_table(bytes: &[u8]) -> Result<Vec<MountContext>, String> {
    let mut mounts = Vec::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if mounts.len() >= MOUNTINFO_ENTRY_CAPACITY {
            return Err(format!(
                "mount table exceeds the {MOUNTINFO_ENTRY_CAPACITY}-entry inspection bound"
            ));
        }
        let fields = line.split(|byte| *byte == b' ').collect::<Vec<_>>();
        let separator = fields
            .iter()
            .position(|field| *field == b"-")
            .ok_or_else(|| "mount table contains a malformed entry".to_owned())?;
        if fields.len() <= 5 || separator + 3 >= fields.len() {
            return Err("mount table contains a truncated entry".to_owned());
        }
        let mount_point = PathBuf::from(unescape_mount_field(fields[4])?);
        let filesystem_type = unescape_mount_field(fields[separator + 1])?;
        let read_only =
            option_present(fields[5], b"ro") || option_present(fields[separator + 3], b"ro");
        mounts.push(MountContext {
            mount_point,
            filesystem_type,
            read_only,
            no_exec: option_present(fields[5], b"noexec")
                || option_present(fields[separator + 3], b"noexec"),
            no_suid: option_present(fields[5], b"nosuid")
                || option_present(fields[separator + 3], b"nosuid"),
            no_dev: option_present(fields[5], b"nodev")
                || option_present(fields[separator + 3], b"nodev"),
        });
    }
    mounts.sort_by_key(|mount| std::cmp::Reverse(mount.mount_point.as_os_str().as_bytes().len()));
    Ok(mounts)
}

fn unescape_mount_field(field: &[u8]) -> Result<OsString, String> {
    let mut output = Vec::with_capacity(field.len());
    let mut index = 0usize;
    while index < field.len() {
        if field[index] == b'\\' && index + 3 < field.len() {
            let digits = &field[index + 1..index + 4];
            if digits.iter().all(|digit| matches!(digit, b'0'..=b'7')) {
                output.push((digits[0] - b'0') * 64 + (digits[1] - b'0') * 8 + digits[2] - b'0');
                index += 4;
                continue;
            }
        }
        output.push(field[index]);
        index += 1;
    }
    if output.contains(&0) {
        return Err("mount table contains a NUL byte".to_owned());
    }
    Ok(OsString::from_vec(output))
}

fn option_present(options: &[u8], expected: &[u8]) -> bool {
    options
        .split(|byte| *byte == b',')
        .any(|item| item == expected)
}

fn find_mount_context<'a>(path: &Path, mounts: &'a [MountContext]) -> Option<&'a MountContext> {
    mounts
        .iter()
        .find(|mount| path.starts_with(&mount.mount_point))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{PermissionsExt, symlink},
        },
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_18r_permission_audit_is_bounded_no_follow_and_revalidated() {
        assert_eq!(
            PermissionAuditRequest::new(Vec::new()),
            Err(PermissionAuditError::InvalidTargetCount)
        );
        assert!(matches!(
            PermissionAuditRequest::new(vec![PathBuf::from("relative")]),
            Err(PermissionAuditError::RelativePath(_))
        ));

        let fixture = tempdir().expect("temporary permission-audit directory");
        let key = fixture
            .path()
            .join(OsString::from_vec(b"id_ed25519-\xff.key".to_vec()));
        fs::write(&key, b"synthetic fixture").expect("write fixture");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o666)).expect("set fixture mode");
        let link = fixture.path().join("key-link");
        symlink(&key, &link).expect("create test symlink");

        let request = PermissionAuditRequest::new(vec![key.clone(), link.clone()])
            .expect("valid audit request");
        let report = audit_permissions(&request, || false).expect("permission audit");
        assert_eq!(report.entries.len(), 2);
        let PermissionAuditEntryState::Inspected(evidence) = &report.entries[0].state else {
            panic!("regular file should be inspected");
        };
        assert!(
            evidence
                .findings
                .iter()
                .any(|finding| finding.kind == PermissionFindingKind::WorldWritable)
        );
        assert!(
            evidence
                .findings
                .iter()
                .any(|finding| finding.kind == PermissionFindingKind::SensitiveNameBroadAccess)
        );
        let fix = evidence.conservative_fix.as_ref().expect("safe mode fix");
        assert_eq!(fix.original_mode, 0o666);
        assert_eq!(fix.proposed_mode, 0o600);
        assert_eq!(
            report.entries[1].state,
            PermissionAuditEntryState::SymbolicLink
        );

        let before = fs::symlink_metadata(&key).expect("initial identity");
        let changed = audit_one_with_hook(&key, before.uid(), &Ok(Vec::new()), || {
            fs::write(&key, b"changed fixture bytes").expect("change fixture");
        });
        assert_eq!(changed.state, PermissionAuditEntryState::Changed);
        assert_eq!(
            audit_permissions(&request, || true),
            Err(PermissionAuditError::Cancelled)
        );
    }

    #[test]
    fn phase_18r_permission_findings_explain_advanced_evidence_without_overclaiming() {
        let evidence = PermissionEvidence {
            identity: PermissionAuditIdentity {
                device: 1,
                inode: 2,
                size: 3,
                mode: 0o7777,
                uid: 42,
                gid: 7,
                modified_seconds: 4,
                modified_nanoseconds: 5,
                changed_seconds: 6,
                changed_nanoseconds: 7,
            },
            object_kind: PermissionObjectKind::RegularFile,
            mode: 0o7777,
            uid: 42,
            gid: 7,
            xattrs: PermissionProbe::Known(XattrSummary {
                total_names: 3,
                user_names: 1,
                security_names: 1,
                access_acl: true,
                default_acl: false,
                linux_capability: true,
            }),
            immutable: PermissionProbe::Known(true),
            mount: PermissionProbe::Known(MountContext {
                mount_point: PathBuf::from("/"),
                filesystem_type: OsString::from("ext4"),
                read_only: false,
                no_exec: false,
                no_suid: false,
                no_dev: false,
            }),
            findings: Vec::new(),
            conservative_fix: None,
        };
        let findings = permission_findings(Path::new("/tmp/.env"), &evidence, 1000);
        for expected in [
            PermissionFindingKind::WorldWritable,
            PermissionFindingKind::GroupWritable,
            PermissionFindingKind::SensitiveNameBroadAccess,
            PermissionFindingKind::SetUserId,
            PermissionFindingKind::SetGroupId,
            PermissionFindingKind::ForeignOwner,
            PermissionFindingKind::AccessControlList,
            PermissionFindingKind::LinuxCapability,
            PermissionFindingKind::Immutable,
        ] {
            assert!(findings.iter().any(|finding| finding.kind == expected));
        }
        assert!(findings.iter().all(|finding| {
            !finding.explanation.contains("malware")
                && !finding.explanation.contains("safe")
                && !finding.explanation.contains("all access")
        }));
        assert_eq!(symbolic_mode(0o7777), "rwsrwsrwt");

        let mounts = parse_mount_table(
            b"36 25 0:32 / /run/media/My\\040Drive rw,nosuid,nodev,noexec - exfat /dev/sdb1 rw\n",
        )
        .expect("parse synthetic mount table");
        assert_eq!(mounts[0].mount_point, Path::new("/run/media/My Drive"));
        assert_eq!(mounts[0].filesystem_type, OsStr::new("exfat"));
        assert!(mounts[0].no_exec && mounts[0].no_suid && mounts[0].no_dev);
    }

    #[test]
    fn phase_18r_permission_audit_reports_bounded_xattr_names_when_supported() {
        let fixture = tempdir().expect("temporary xattr directory");
        let path = fixture.path().join("report.txt");
        fs::write(&path, b"report").expect("write xattr fixture");
        match rustix::fs::lsetxattr(
            &path,
            "user.floe.phase18r",
            b"value-is-never-read-by-audit",
            rustix::fs::XattrFlags::empty(),
        ) {
            Ok(()) => {
                let request = PermissionAuditRequest::new(vec![path]).expect("audit request");
                let report = audit_permissions(&request, || false).expect("audit xattrs");
                let PermissionAuditEntryState::Inspected(evidence) = &report.entries[0].state
                else {
                    panic!("xattr fixture should be inspected");
                };
                let PermissionProbe::Known(summary) = &evidence.xattrs else {
                    panic!("supported xattrs should be reported");
                };
                assert!(summary.total_names >= 1);
                assert!(summary.user_names >= 1);
            }
            Err(Errno::NOTSUP) => {}
            Err(error) => panic!("unexpected xattr fixture failure: {error}"),
        }
    }
}
