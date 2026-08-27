//! GTK-independent integrity-baseline and monitoring policy.
//!
//! This module owns only immutable baseline values, deterministic comparisons,
//! and bounded event/rescan state. It performs no filesystem access and makes
//! no intrusion-detection claim.

use std::{
    collections::HashMap,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

pub const INTEGRITY_MONITOR_ENTRY_CAPACITY: usize = 2_048;
pub const INTEGRITY_MONITOR_EVENT_CAPACITY: usize = 16_384;
pub const INTEGRITY_MONITOR_PATH_BYTES: usize = 4 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityBaselineEntry {
    path: PathBuf,
    sha256: String,
}

impl IntegrityBaselineEntry {
    pub fn new(path: PathBuf, sha256: String) -> Result<Self, IntegrityBaselineError> {
        validate_relative_path(&path)?;
        if !is_sha256(&sha256) {
            return Err(IntegrityBaselineError::InvalidDigest);
        }
        Ok(Self { path, sha256 })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityBaseline {
    root: PathBuf,
    entries: Vec<IntegrityBaselineEntry>,
}

impl IntegrityBaseline {
    pub fn new(
        root: PathBuf,
        mut entries: Vec<IntegrityBaselineEntry>,
    ) -> Result<Self, IntegrityBaselineError> {
        validate_root(&root)?;
        if entries.len() > INTEGRITY_MONITOR_ENTRY_CAPACITY {
            return Err(IntegrityBaselineError::TooManyEntries);
        }
        entries.sort_by(|left, right| raw_cmp(left.path(), right.path()));
        if entries
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err(IntegrityBaselineError::DuplicatePath);
        }
        Ok(Self { root, entries })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[IntegrityBaselineEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityEntryStatus {
    Matching,
    Changed { expected: String, actual: String },
    Missing,
    New,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityDiffEntry {
    path: PathBuf,
    status: IntegrityEntryStatus,
}

impl IntegrityDiffEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn status(&self) -> &IntegrityEntryStatus {
        &self.status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityBaselineDiff {
    entries: Vec<IntegrityDiffEntry>,
}

impl IntegrityBaselineDiff {
    pub fn between(
        baseline: &IntegrityBaseline,
        current: &[IntegrityBaselineEntry],
    ) -> Result<Self, IntegrityBaselineError> {
        if current.len() > INTEGRITY_MONITOR_ENTRY_CAPACITY {
            return Err(IntegrityBaselineError::TooManyEntries);
        }
        let mut current_by_path = HashMap::with_capacity(current.len());
        for entry in current {
            validate_relative_path(entry.path())?;
            if !is_sha256(entry.sha256()) {
                return Err(IntegrityBaselineError::InvalidDigest);
            }
            if current_by_path.insert(entry.path(), entry).is_some() {
                return Err(IntegrityBaselineError::DuplicatePath);
            }
        }

        let baseline_by_path = baseline
            .entries()
            .iter()
            .map(|entry| (entry.path(), entry))
            .collect::<HashMap<_, _>>();
        let mut entries = Vec::with_capacity(baseline.entries().len() + current.len());
        for expected in baseline.entries() {
            let status = match current_by_path.get(expected.path()) {
                Some(actual) if actual.sha256() == expected.sha256() => {
                    IntegrityEntryStatus::Matching
                }
                Some(actual) => IntegrityEntryStatus::Changed {
                    expected: expected.sha256().to_owned(),
                    actual: actual.sha256().to_owned(),
                },
                None => IntegrityEntryStatus::Missing,
            };
            entries.push(IntegrityDiffEntry {
                path: expected.path().to_path_buf(),
                status,
            });
        }
        for actual in current {
            if !baseline_by_path.contains_key(actual.path()) {
                entries.push(IntegrityDiffEntry {
                    path: actual.path().to_path_buf(),
                    status: IntegrityEntryStatus::New,
                });
            }
        }
        entries.sort_by(|left, right| raw_cmp(left.path(), right.path()));
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[IntegrityDiffEntry] {
        &self.entries
    }

    pub fn has_changes(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.status != IntegrityEntryStatus::Matching)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityMonitorStaleReason {
    ChangesObserved,
    WatcherOverflow,
    WatcherInvalidated,
    OfflineGap,
    MountLost,
    RootUnavailable,
    ScanInterrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityMonitorStatus {
    Disabled,
    Paused,
    Idle,
    Scanning,
    Current,
    NeedsRecheck(IntegrityMonitorStaleReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityWatchEvent {
    Changed(PathBuf),
    Created(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    Overflow,
    Invalidated,
    Offline,
    MountLost,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityWatchSetPolicy {
    root: PathBuf,
}

impl IntegrityWatchSetPolicy {
    pub fn new(root: PathBuf) -> Result<Self, IntegrityMonitorPolicyError> {
        validate_root(&root).map_err(IntegrityMonitorPolicyError::Baseline)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn accepts(&self, event: &IntegrityWatchEvent) -> bool {
        match event {
            IntegrityWatchEvent::Changed(path)
            | IntegrityWatchEvent::Created(path)
            | IntegrityWatchEvent::Deleted(path) => self.accepts_path(path),
            IntegrityWatchEvent::Renamed { from, to } => {
                self.accepts_path(from) && self.accepts_path(to)
            }
            IntegrityWatchEvent::Overflow
            | IntegrityWatchEvent::Invalidated
            | IntegrityWatchEvent::Offline
            | IntegrityWatchEvent::MountLost => true,
        }
    }

    fn accepts_path(&self, path: &Path) -> bool {
        path.as_os_str().as_bytes().len() <= INTEGRITY_MONITOR_PATH_BYTES
            && path.is_absolute()
            && is_lexically_normal(path)
            && path.starts_with(&self.root)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityRescanDecision {
    None,
    Pending,
    Start { generation: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityMonitorSession {
    enabled: bool,
    paused: bool,
    generation: u64,
    scan_in_flight: bool,
    rescan_pending: bool,
    event_count: usize,
    status: IntegrityMonitorStatus,
    stale_reason: Option<IntegrityMonitorStaleReason>,
}

impl Default for IntegrityMonitorSession {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            generation: 0,
            scan_in_flight: false,
            rescan_pending: false,
            event_count: 0,
            status: IntegrityMonitorStatus::Disabled,
            stale_reason: None,
        }
    }
}

impl IntegrityMonitorSession {
    pub fn enable(&mut self) {
        self.enabled = true;
        self.paused = false;
        self.status = IntegrityMonitorStatus::Idle;
        self.rescan_pending = false;
        self.event_count = 0;
        self.stale_reason = None;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.paused = false;
        self.scan_in_flight = false;
        self.rescan_pending = false;
        self.event_count = 0;
        self.stale_reason = None;
        self.generation = next_generation(self.generation);
        self.status = IntegrityMonitorStatus::Disabled;
    }

    pub fn pause(&mut self) {
        if !self.enabled {
            return;
        }
        self.paused = true;
        self.scan_in_flight = false;
        self.generation = next_generation(self.generation);
        self.rescan_pending = true;
        self.stale_reason = Some(IntegrityMonitorStaleReason::OfflineGap);
        self.status = IntegrityMonitorStatus::Paused;
    }

    pub fn resume(&mut self) {
        if !self.enabled || !self.paused {
            return;
        }
        self.paused = false;
        self.rescan_pending = true;
        self.stale_reason = Some(IntegrityMonitorStaleReason::OfflineGap);
        self.status = IntegrityMonitorStatus::NeedsRecheck(IntegrityMonitorStaleReason::OfflineGap);
    }

    pub fn record_event(
        &mut self,
        policy: &IntegrityWatchSetPolicy,
        event: &IntegrityWatchEvent,
    ) -> Result<IntegrityRescanDecision, IntegrityMonitorPolicyError> {
        if !policy.accepts(event) {
            return Err(IntegrityMonitorPolicyError::OutsideWatchSet);
        }
        if !self.enabled || self.paused {
            return Ok(IntegrityRescanDecision::None);
        }
        self.event_count = self.event_count.saturating_add(1);
        let reason = match event {
            IntegrityWatchEvent::Overflow => IntegrityMonitorStaleReason::WatcherOverflow,
            IntegrityWatchEvent::Invalidated => IntegrityMonitorStaleReason::WatcherInvalidated,
            IntegrityWatchEvent::Offline => IntegrityMonitorStaleReason::OfflineGap,
            IntegrityWatchEvent::MountLost => IntegrityMonitorStaleReason::MountLost,
            _ if self.event_count > INTEGRITY_MONITOR_EVENT_CAPACITY => {
                IntegrityMonitorStaleReason::WatcherOverflow
            }
            _ => IntegrityMonitorStaleReason::ChangesObserved,
        };
        self.rescan_pending = true;
        self.stale_reason = Some(prefer_stale_reason(self.stale_reason, reason));
        self.status = IntegrityMonitorStatus::NeedsRecheck(
            self.stale_reason.expect("stale reason was just recorded"),
        );
        Ok(IntegrityRescanDecision::Pending)
    }

    /// Called after the application-side debounce expires.
    pub fn take_rescan(&mut self) -> IntegrityRescanDecision {
        if !self.enabled || self.paused || self.scan_in_flight || !self.rescan_pending {
            return IntegrityRescanDecision::None;
        }
        self.rescan_pending = false;
        self.event_count = 0;
        self.scan_in_flight = true;
        self.generation = next_generation(self.generation);
        self.status = IntegrityMonitorStatus::Scanning;
        IntegrityRescanDecision::Start {
            generation: self.generation,
        }
    }

    pub fn request_explicit_scan(&mut self) -> IntegrityRescanDecision {
        if !self.enabled || self.paused {
            return IntegrityRescanDecision::None;
        }
        self.rescan_pending = true;
        if self.scan_in_flight {
            return IntegrityRescanDecision::Pending;
        }
        self.take_rescan()
    }

    pub fn complete_scan(&mut self, generation: u64) -> IntegrityRescanDecision {
        if !self.enabled || self.paused || !self.scan_in_flight || generation != self.generation {
            return IntegrityRescanDecision::None;
        }
        self.scan_in_flight = false;
        if self.rescan_pending {
            let reason = self
                .stale_reason
                .unwrap_or(IntegrityMonitorStaleReason::ChangesObserved);
            self.status = IntegrityMonitorStatus::NeedsRecheck(reason);
            IntegrityRescanDecision::Pending
        } else {
            self.stale_reason = None;
            self.status = IntegrityMonitorStatus::Current;
            IntegrityRescanDecision::None
        }
    }

    pub fn interrupt_scan(&mut self, reason: IntegrityMonitorStaleReason) {
        if !self.enabled {
            return;
        }
        self.scan_in_flight = false;
        self.rescan_pending = true;
        self.stale_reason = Some(reason);
        self.generation = next_generation(self.generation);
        self.status = IntegrityMonitorStatus::NeedsRecheck(reason);
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub const fn rescan_pending(&self) -> bool {
        self.rescan_pending
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn status(&self) -> IntegrityMonitorStatus {
        self.status
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IntegrityBaselineError {
    #[error("integrity baseline root must be an absolute normalized path")]
    InvalidRoot,
    #[error("integrity baseline member must be a normalized relative path")]
    InvalidPath,
    #[error("integrity baseline path exceeds the byte limit")]
    PathTooLong,
    #[error("integrity baseline SHA-256 digest is invalid")]
    InvalidDigest,
    #[error("integrity baseline exceeds the entry limit")]
    TooManyEntries,
    #[error("integrity baseline contains a duplicate path")]
    DuplicatePath,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IntegrityMonitorPolicyError {
    #[error(transparent)]
    Baseline(IntegrityBaselineError),
    #[error("integrity watcher event is outside its exact watched root")]
    OutsideWatchSet,
}

fn validate_root(path: &Path) -> Result<(), IntegrityBaselineError> {
    if !path.is_absolute() || !is_lexically_normal(path) {
        return Err(IntegrityBaselineError::InvalidRoot);
    }
    if path.as_os_str().as_bytes().len() > INTEGRITY_MONITOR_PATH_BYTES {
        return Err(IntegrityBaselineError::PathTooLong);
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), IntegrityBaselineError> {
    if path.as_os_str().is_empty() || path.is_absolute() || !is_lexically_normal(path) {
        return Err(IntegrityBaselineError::InvalidPath);
    }
    if path.as_os_str().as_bytes().len() > INTEGRITY_MONITOR_PATH_BYTES {
        return Err(IntegrityBaselineError::PathTooLong);
    }
    Ok(())
}

fn is_lexically_normal(path: &Path) -> bool {
    !path.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn raw_cmp(left: &Path, right: &Path) -> std::cmp::Ordering {
    left.as_os_str()
        .as_bytes()
        .cmp(right.as_os_str().as_bytes())
}

fn next_generation(generation: u64) -> u64 {
    generation.wrapping_add(1).max(1)
}

fn prefer_stale_reason(
    current: Option<IntegrityMonitorStaleReason>,
    incoming: IntegrityMonitorStaleReason,
) -> IntegrityMonitorStaleReason {
    fn priority(reason: IntegrityMonitorStaleReason) -> u8 {
        match reason {
            IntegrityMonitorStaleReason::ChangesObserved => 0,
            IntegrityMonitorStaleReason::ScanInterrupted => 1,
            IntegrityMonitorStaleReason::OfflineGap => 2,
            IntegrityMonitorStaleReason::WatcherOverflow => 3,
            IntegrityMonitorStaleReason::WatcherInvalidated => 4,
            IntegrityMonitorStaleReason::RootUnavailable => 5,
            IntegrityMonitorStaleReason::MountLost => 6,
        }
    }
    current
        .filter(|reason| priority(*reason) >= priority(incoming))
        .unwrap_or(incoming)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    fn entry(path: Vec<u8>, digest_byte: u8) -> IntegrityBaselineEntry {
        IntegrityBaselineEntry::new(
            PathBuf::from(OsString::from_vec(path)),
            format!("{digest_byte:02x}").repeat(32),
        )
        .expect("valid entry")
    }

    #[test]
    fn phase_18u_baseline_diff_is_raw_path_safe_and_deterministic() {
        let baseline = IntegrityBaseline::new(
            PathBuf::from("/monitored"),
            vec![
                entry(b"missing".to_vec(), 1),
                entry(b"changed".to_vec(), 2),
                entry(b"matching-\xff".to_vec(), 3),
            ],
        )
        .expect("baseline");
        let current = vec![
            entry(b"new".to_vec(), 4),
            entry(b"matching-\xff".to_vec(), 3),
            entry(b"changed".to_vec(), 5),
        ];

        let diff = IntegrityBaselineDiff::between(&baseline, &current).expect("diff");
        assert_eq!(
            diff.entries()
                .iter()
                .map(|entry| entry.path().as_os_str().as_bytes().to_vec())
                .collect::<Vec<_>>(),
            vec![
                b"changed".to_vec(),
                b"matching-\xff".to_vec(),
                b"missing".to_vec(),
                b"new".to_vec(),
            ]
        );
        assert!(matches!(
            diff.entries()[0].status(),
            IntegrityEntryStatus::Changed { .. }
        ));
        assert_eq!(diff.entries()[1].status(), &IntegrityEntryStatus::Matching);
        assert_eq!(diff.entries()[2].status(), &IntegrityEntryStatus::Missing);
        assert_eq!(diff.entries()[3].status(), &IntegrityEntryStatus::New);
    }

    #[test]
    fn phase_18u_monitor_coalesces_storm_and_requires_full_rescan_after_gap() {
        let policy = IntegrityWatchSetPolicy::new(PathBuf::from("/monitored")).expect("policy");
        let mut session = IntegrityMonitorSession::default();
        assert_eq!(session.status(), IntegrityMonitorStatus::Disabled);
        session.enable();

        for _ in 0..=INTEGRITY_MONITOR_EVENT_CAPACITY {
            assert_eq!(
                session
                    .record_event(
                        &policy,
                        &IntegrityWatchEvent::Changed(PathBuf::from("/monitored/item")),
                    )
                    .expect("event"),
                IntegrityRescanDecision::Pending
            );
        }
        assert_eq!(
            session.status(),
            IntegrityMonitorStatus::NeedsRecheck(IntegrityMonitorStaleReason::WatcherOverflow)
        );
        let IntegrityRescanDecision::Start { generation } = session.take_rescan() else {
            panic!("storm starts exactly one scan")
        };
        session
            .record_event(&policy, &IntegrityWatchEvent::Offline)
            .expect("offline event");
        assert_eq!(
            session.complete_scan(generation),
            IntegrityRescanDecision::Pending
        );
        assert!(session.rescan_pending());
    }

    #[test]
    fn phase_18u_monitor_pause_disable_and_generation_reject_stale_completion() {
        let mut session = IntegrityMonitorSession::default();
        session.enable();
        let IntegrityRescanDecision::Start { generation } = session.request_explicit_scan() else {
            panic!("explicit scan starts")
        };
        session.pause();
        assert_eq!(session.status(), IntegrityMonitorStatus::Paused);
        assert_eq!(
            session.complete_scan(generation),
            IntegrityRescanDecision::None
        );
        session.resume();
        assert_eq!(
            session.status(),
            IntegrityMonitorStatus::NeedsRecheck(IntegrityMonitorStaleReason::OfflineGap)
        );
        session.disable();
        assert!(!session.enabled());
        assert!(!session.rescan_pending());
    }

    #[test]
    fn phase_18u_monitor_watch_set_rejects_outside_or_non_normal_paths() {
        let policy = IntegrityWatchSetPolicy::new(PathBuf::from("/monitored")).expect("policy");
        assert!(policy.accepts(&IntegrityWatchEvent::Created(PathBuf::from(
            "/monitored/child"
        ))));
        assert!(!policy.accepts(&IntegrityWatchEvent::Changed(PathBuf::from(
            "/elsewhere/child"
        ))));
        assert!(!policy.accepts(&IntegrityWatchEvent::Changed(PathBuf::from(
            "/monitored/../elsewhere"
        ))));
        assert!(!policy.accepts(&IntegrityWatchEvent::Renamed {
            from: PathBuf::from("/monitored/old"),
            to: PathBuf::from("/elsewhere/new"),
        }));
    }

    #[test]
    fn phase_18u_monitor_gap_events_have_explicit_deterministic_stale_reasons() {
        let policy = IntegrityWatchSetPolicy::new(PathBuf::from("/monitored")).expect("policy");
        for (event, reason) in [
            (
                IntegrityWatchEvent::Overflow,
                IntegrityMonitorStaleReason::WatcherOverflow,
            ),
            (
                IntegrityWatchEvent::Invalidated,
                IntegrityMonitorStaleReason::WatcherInvalidated,
            ),
            (
                IntegrityWatchEvent::Offline,
                IntegrityMonitorStaleReason::OfflineGap,
            ),
            (
                IntegrityWatchEvent::MountLost,
                IntegrityMonitorStaleReason::MountLost,
            ),
        ] {
            let mut session = IntegrityMonitorSession::default();
            session.enable();
            session.record_event(&policy, &event).expect("event");
            assert_eq!(
                session.status(),
                IntegrityMonitorStatus::NeedsRecheck(reason)
            );
            assert!(session.rescan_pending());
        }
    }
}
