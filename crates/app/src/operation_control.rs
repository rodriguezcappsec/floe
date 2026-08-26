use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
    time::Duration,
};

use floe_core::{JobProgress, ProgressUnit};

const MIN_RATE_SAMPLE: Duration = Duration::from_millis(200);
const RATE_ALPHA: f64 = 0.25;
pub const MAX_KEEP_BOTH_ATTEMPTS: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BatchId(u64);

impl BatchId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchStatus {
    Queued,
    Running,
    Pausing,
    Paused,
    Cancelling,
    Completed,
    CompletedWithIssues,
    Cancelled,
}

impl BatchStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::CompletedWithIssues | Self::Cancelled
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchSnapshot {
    id: BatchId,
    status: BatchStatus,
    total: usize,
    completed: usize,
    skipped: usize,
    failed: usize,
    cancelled: usize,
    active: bool,
}

impl BatchSnapshot {
    #[allow(
        clippy::too_many_arguments,
        reason = "the immutable snapshot names each independent batch counter explicitly"
    )]
    pub const fn new(
        id: BatchId,
        status: BatchStatus,
        total: usize,
        completed: usize,
        skipped: usize,
        failed: usize,
        cancelled: usize,
        active: bool,
    ) -> Self {
        Self {
            id,
            status,
            total,
            completed,
            skipped,
            failed,
            cancelled,
            active,
        }
    }

    pub const fn id(self) -> BatchId {
        self.id
    }

    pub const fn status(self) -> BatchStatus {
        self.status
    }

    pub const fn total(self) -> usize {
        self.total
    }

    pub const fn completed(self) -> usize {
        self.completed
    }

    pub const fn skipped(self) -> usize {
        self.skipped
    }

    pub const fn failed(self) -> usize {
        self.failed
    }

    pub const fn cancelled(self) -> usize {
        self.cancelled
    }

    pub const fn processed(self) -> usize {
        self.completed
            .saturating_add(self.skipped)
            .saturating_add(self.failed)
            .saturating_add(self.cancelled)
    }

    pub const fn remaining(self) -> usize {
        self.total
            .saturating_sub(self.processed().saturating_add(self.active as usize))
    }

    pub const fn active(self) -> bool {
        self.active
    }

    pub const fn current_item(self) -> Option<usize> {
        if self.active {
            Some(self.processed().saturating_add(1))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferEstimate {
    bytes_per_second: u64,
    eta: Duration,
}

impl TransferEstimate {
    pub const fn bytes_per_second(self) -> u64 {
        self.bytes_per_second
    }

    pub const fn eta(self) -> Duration {
        self.eta
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TransferTelemetry {
    previous_elapsed: Option<Duration>,
    previous_completed: u64,
    smoothed_bytes_per_second: Option<f64>,
}

impl TransferTelemetry {
    pub fn observe(
        &mut self,
        elapsed: Duration,
        progress: JobProgress,
    ) -> Option<TransferEstimate> {
        if progress.unit() != ProgressUnit::Bytes {
            self.reset();
            return None;
        }
        let Some(total) = progress.total() else {
            self.reset();
            return None;
        };
        let completed = progress.completed();
        let Some(previous_elapsed) = self.previous_elapsed else {
            self.previous_elapsed = Some(elapsed);
            self.previous_completed = completed;
            return None;
        };
        if elapsed <= previous_elapsed || completed < self.previous_completed {
            self.previous_elapsed = Some(elapsed);
            self.previous_completed = completed;
            self.smoothed_bytes_per_second = None;
            return None;
        }
        let elapsed_delta = elapsed - previous_elapsed;
        let byte_delta = completed - self.previous_completed;
        if elapsed_delta < MIN_RATE_SAMPLE || byte_delta == 0 {
            return None;
        }
        self.previous_elapsed = Some(elapsed);
        self.previous_completed = completed;

        let instantaneous = byte_delta as f64 / elapsed_delta.as_secs_f64();
        if !instantaneous.is_finite() || instantaneous <= 0.0 {
            return None;
        }
        let smoothed = self
            .smoothed_bytes_per_second
            .map_or(instantaneous, |previous| {
                previous + RATE_ALPHA * (instantaneous - previous)
            });
        self.smoothed_bytes_per_second = Some(smoothed);
        let remaining = total.saturating_sub(completed);
        if remaining == 0 {
            return None;
        }
        let eta_seconds = remaining as f64 / smoothed;
        if !eta_seconds.is_finite() || eta_seconds <= 0.0 {
            return None;
        }
        Some(TransferEstimate {
            bytes_per_second: smoothed.round().clamp(1.0, u64::MAX as f64) as u64,
            eta: Duration::from_secs_f64(eta_seconds),
        })
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub fn keep_both_name(original: &OsStr, attempt: u32) -> Option<OsString> {
    if attempt == 0 || attempt > MAX_KEEP_BOTH_ATTEMPTS {
        return None;
    }
    let path = Path::new(original);
    let stem = path.file_stem()?;
    let extension = path.extension();
    let mut bytes = stem.as_bytes().to_vec();
    if attempt == 1 {
        bytes.extend_from_slice(b" (copy)");
    } else {
        bytes.extend_from_slice(format!(" (copy {attempt})").as_bytes());
    }
    if let Some(extension) = extension {
        bytes.push(b'.');
        bytes.extend_from_slice(extension.as_bytes());
    }
    Some(OsString::from_vec(bytes))
}

pub fn duplicate_name(original: &OsStr, attempt: u32) -> Option<OsString> {
    if attempt == 0 || attempt > MAX_KEEP_BOTH_ATTEMPTS {
        return None;
    }
    let path = Path::new(original);
    let stem = path.file_stem()?;
    let extension = path.extension();
    let (base, existing_copy) = duplicate_stem(stem.as_bytes());
    let ordinal = existing_copy.checked_add(attempt)?;
    if ordinal > MAX_KEEP_BOTH_ATTEMPTS {
        return None;
    }
    let mut bytes = base.to_vec();
    if ordinal == 1 {
        bytes.extend_from_slice(b" (copy)");
    } else {
        bytes.extend_from_slice(format!(" (copy {ordinal})").as_bytes());
    }
    if let Some(extension) = extension {
        bytes.push(b'.');
        bytes.extend_from_slice(extension.as_bytes());
    }
    Some(OsString::from_vec(bytes))
}

fn duplicate_stem(stem: &[u8]) -> (&[u8], u32) {
    if let Some(base) = stem.strip_suffix(b" (copy)") {
        return (base, 1);
    }
    let Some(without_close) = stem.strip_suffix(b")") else {
        return (stem, 0);
    };
    let Some(marker) = without_close
        .windows(b" (copy ".len())
        .rposition(|window| window == b" (copy ")
    else {
        return (stem, 0);
    };
    let digits = &without_close[marker + b" (copy ".len()..];
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return (stem, 0);
    }
    let Ok(number) = std::str::from_utf8(digits)
        .unwrap_or_default()
        .parse::<u32>()
    else {
        return (stem, 0);
    };
    if !(2..=MAX_KEEP_BOTH_ATTEMPTS).contains(&number) {
        return (stem, 0);
    }
    (&stem[..marker], number)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    #[test]
    fn phase_6p_telemetry_reports_only_meaningful_byte_samples() {
        let mut telemetry = TransferTelemetry::default();
        assert_eq!(
            telemetry.observe(
                Duration::ZERO,
                JobProgress::bytes(0, Some(1_000)).expect("progress should be valid")
            ),
            None
        );
        assert_eq!(
            telemetry.observe(
                Duration::from_millis(100),
                JobProgress::bytes(100, Some(1_000)).expect("progress should be valid")
            ),
            None
        );
        let estimate = telemetry
            .observe(
                Duration::from_millis(300),
                JobProgress::bytes(300, Some(1_000)).expect("progress should be valid"),
            )
            .expect("second meaningful byte sample should estimate rate");
        assert_eq!(estimate.bytes_per_second(), 1_000);
        assert_eq!(estimate.eta(), Duration::from_millis(700));

        assert_eq!(
            telemetry.observe(
                Duration::from_millis(600),
                JobProgress::items(2, Some(4)).expect("item progress should be valid")
            ),
            None
        );
    }

    #[test]
    fn phase_6p_telemetry_accumulates_frequent_samples_until_rate_is_meaningful() {
        let mut telemetry = TransferTelemetry::default();
        assert_eq!(
            telemetry.observe(
                Duration::ZERO,
                JobProgress::bytes(0, Some(1_000)).expect("starting progress")
            ),
            None
        );
        for (millis, bytes) in [(50, 50), (100, 100), (150, 150)] {
            assert_eq!(
                telemetry.observe(
                    Duration::from_millis(millis),
                    JobProgress::bytes(bytes, Some(1_000)).expect("frequent progress")
                ),
                None
            );
        }
        let estimate = telemetry
            .observe(
                Duration::from_millis(250),
                JobProgress::bytes(250, Some(1_000)).expect("accumulated progress"),
            )
            .expect("samples should accumulate from the last meaningful baseline");
        assert_eq!(estimate.bytes_per_second(), 1_000);
        assert_eq!(estimate.eta(), Duration::from_millis(750));
    }

    #[test]
    fn phase_6p_telemetry_resets_on_regression_and_suppresses_completion() {
        let mut telemetry = TransferTelemetry::default();
        let first = JobProgress::bytes(500, Some(1_000)).expect("progress should be valid");
        let regressed = JobProgress::bytes(400, Some(1_000)).expect("progress should be valid");
        let complete = JobProgress::bytes(1_000, Some(1_000)).expect("progress should be valid");
        assert_eq!(telemetry.observe(Duration::ZERO, first), None);
        assert_eq!(telemetry.observe(Duration::from_secs(1), regressed), None);
        assert_eq!(telemetry.observe(Duration::from_secs(2), complete), None);
    }

    #[test]
    fn phase_6p_conflict_keep_both_names_preserve_raw_identity() {
        assert_eq!(
            keep_both_name(OsStr::new("report.txt"), 1),
            Some(OsString::from("report (copy).txt"))
        );
        assert_eq!(
            keep_both_name(OsStr::new("report.txt"), 2),
            Some(OsString::from("report (copy 2).txt"))
        );
        let raw = OsString::from_vec(b"raw-\xff.bin".to_vec());
        assert_eq!(
            keep_both_name(&raw, 2)
                .expect("raw name should remain representable")
                .as_bytes(),
            b"raw-\xff (copy 2).bin"
        );
        assert_eq!(keep_both_name(OsStr::new("item"), 0), None);
        assert_eq!(
            keep_both_name(OsStr::new("item"), MAX_KEEP_BOTH_ATTEMPTS + 1),
            None
        );
    }

    #[test]
    fn phase_6q_duplicate_names_are_bounded_and_preserve_raw_identity() {
        let original = OsString::from_vec(b"report-\xff.txt".to_vec());
        assert_eq!(
            duplicate_name(&original, 1)
                .expect("first duplicate")
                .as_bytes(),
            b"report-\xff (copy).txt"
        );
        assert_eq!(
            duplicate_name(&original, 2)
                .expect("second duplicate")
                .as_bytes(),
            b"report-\xff (copy 2).txt"
        );
        assert_eq!(duplicate_name(&original, 0), None);
        assert_eq!(duplicate_name(&original, MAX_KEEP_BOTH_ATTEMPTS + 1), None);
    }

    #[test]
    fn phase_12e_duplicate_suffixes_progress_without_stacking_and_preserve_raw_extensions() {
        assert_eq!(
            duplicate_name(OsStr::new("report (copy).txt"), 1),
            Some(OsString::from("report (copy 2).txt"))
        );
        assert_eq!(
            duplicate_name(OsStr::new("report (copy 7).txt"), 2),
            Some(OsString::from("report (copy 9).txt"))
        );
        assert_eq!(
            duplicate_name(OsStr::new("report (copy nope).txt"), 1),
            Some(OsString::from("report (copy nope) (copy).txt"))
        );
        let raw = OsString::from_vec(b"raw-\xff (copy 2).bin".to_vec());
        assert_eq!(
            duplicate_name(&raw, 1).expect("raw duplicate").as_bytes(),
            b"raw-\xff (copy 3).bin"
        );
        assert_eq!(duplicate_name(OsStr::new("item (copy 10000)"), 1), None);
    }

    #[test]
    fn phase_6p_batch_snapshot_never_underflows_remaining_items() {
        let id = BatchId::new(1).expect("non-zero batch ID should be valid");
        let snapshot =
            BatchSnapshot::new(id, BatchStatus::CompletedWithIssues, 3, 1, 1, 1, 0, false);
        assert_eq!(snapshot.processed(), 3);
        assert_eq!(snapshot.remaining(), 0);
        assert_eq!(snapshot.current_item(), None);
    }
}
