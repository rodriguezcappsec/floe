//! Opt-in release performance coverage for Phase 21A.
//!
//! This is deliberately a test-only harness: it exercises production Floe
//! implementations while keeping every generated file below one `tempfile`
//! root. Run it serially in release mode; the ordinary test suite never pays
//! the 100,000-file fixture cost.

use std::{
    fs::{self, File},
    hint::black_box,
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use floe_core::{
    ChecksumAlgorithm, ChecksumRequest, ConflictPolicy, ContentSearchLimits, ContentSearchRequest,
    CopyCancellation, CopyRequest, DirectorySort, DuplicateHashError, DuplicateHashResult,
    DuplicateScanLimits, DuplicateScanRequest, FilenameSearchLimits, FilenameSearchRequest,
    FilenameSearchScope, FolderFilterMode, FolderFilterPattern, SortColumn, SortDirection,
    SymlinkPolicy, enumerate_directory, execute_copy, find_duplicates, search_filenames,
};
use image::{ImageBuffer, Rgba};
use tempfile::tempdir;

use crate::{
    checksum_executor::execute_checksum,
    integrity::{FingerprintVerification, save_fingerprint, verify_fingerprint},
    sort_metadata_index::{count_text_facts, index_and_sort_for_performance},
    thumbnail::{ThumbnailKey, decode_thumbnail},
};

const HUGE_ENTRY_COUNT: usize = 100_000;

fn measure<T>(name: &str, budget: Duration, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = work();
    let elapsed = started.elapsed();
    let status = if elapsed <= budget { "pass" } else { "fail" };
    println!(
        "PHASE21A_RESULT workload={name} elapsed_ms={} budget_ms={} status={status}",
        elapsed.as_millis(),
        budget.as_millis()
    );
    assert!(
        elapsed <= budget,
        "{name} exceeded its {:?} release budget with {:?}",
        budget,
        elapsed
    );
    value
}

fn write_repeated(path: &Path, bytes: usize, seed: u8) {
    let mut file = File::create(path).expect("performance fixture file should be created");
    let block = vec![seed; 64 * 1024];
    let mut remaining = bytes;
    while remaining > 0 {
        let count = remaining.min(block.len());
        file.write_all(&block[..count])
            .expect("performance fixture bytes should be written");
        remaining -= count;
    }
}

fn digest_bytes(path: &Path) -> Result<[u8; 32], DuplicateHashError> {
    let request = ChecksumRequest::new(vec![path.to_path_buf()], ChecksumAlgorithm::Sha256, None)
        .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
    let outcome = execute_checksum(&request, || false, |_, _| {})
        .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
    let digest = outcome
        .items
        .first()
        .ok_or_else(|| DuplicateHashError::Failed("checksum produced no item".to_owned()))?
        .digest
        .as_bytes();
    if digest.len() != 64 {
        return Err(DuplicateHashError::Failed(
            "SHA-256 digest had an invalid length".to_owned(),
        ));
    }
    let mut output = [0_u8; 32];
    for (index, pair) in digest.chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair)
            .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
        output[index] = u8::from_str_radix(text, 16)
            .map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
    }
    Ok(output)
}

fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_ascii_whitespace().nth(1)?.parse().ok()
}

#[test]
#[ignore = "release-only 100,000-file performance harness"]
fn phase_21a_performance() {
    let fixture = tempdir().expect("isolated Phase 21A root should be created");
    let huge = fixture.path().join("directory-100k");
    fs::create_dir(&huge).expect("100k fixture directory should be created");

    measure("fixture_create_100k", Duration::from_secs(120), || {
        for index in 0..HUGE_ENTRY_COUNT {
            let marker = if index % 1_000 == 0 {
                "Needle"
            } else {
                "ordinary"
            };
            File::create(huge.join(format!("entry-{index:06}-{marker}.txt")))
                .expect("100k fixture entry should be created");
        }
    });

    let listing = measure("enumerate_100k", Duration::from_secs(30), || {
        enumerate_directory(&huge).expect("100k directory should enumerate")
    });
    assert_eq!(listing.entries().len(), HUGE_ENTRY_COUNT);
    let mut entries = listing.into_entries();

    measure("metadata_sort_100k", Duration::from_secs(5), || {
        DirectorySort::new(SortColumn::Size, SortDirection::Descending).sort_entries(&mut entries);
    });
    assert_eq!(entries.len(), HUGE_ENTRY_COUNT);

    let matcher = FolderFilterPattern::compile(FolderFilterMode::Text, "needle")
        .expect("performance filter should compile");
    let current_matches = measure("quick_filter_100k", Duration::from_secs(5), || {
        entries
            .iter()
            .filter(|entry| matcher.matches(entry.display_name()))
            .count()
    });
    assert_eq!(current_matches, 100);

    let request = FilenameSearchRequest::new(
        huge.clone(),
        "needle".to_owned(),
        FilenameSearchScope::CurrentFolder,
        false,
    )
    .expect("filename search request should be valid");
    let mut filename_matches = 0_usize;
    let summary = measure("filename_search_100k", Duration::from_secs(30), || {
        search_filenames(
            &request,
            FilenameSearchLimits::default(),
            || false,
            |batch, _| {
                filename_matches += batch.len();
                true
            },
        )
        .expect("100k filename search should complete")
    });
    assert_eq!(filename_matches, 100);
    assert_eq!(summary.examined_entries, HUGE_ENTRY_COUNT);

    let images = fixture.path().join("thumbnails");
    fs::create_dir(&images).expect("thumbnail fixture directory should be created");
    for index in 0..32_u32 {
        let image = ImageBuffer::from_fn(512, 512, |x, y| {
            Rgba([
                (x.wrapping_add(index) % 256) as u8,
                (y.wrapping_add(index) % 256) as u8,
                ((x ^ y) % 256) as u8,
                255,
            ])
        });
        image
            .save(images.join(format!("image-{index:02}.png")))
            .expect("thumbnail image should be encoded");
    }
    let image_entries = enumerate_directory(&images)
        .expect("thumbnail fixtures should enumerate")
        .into_entries();
    measure("thumbnails_32", Duration::from_secs(15), || {
        for entry in &image_entries {
            let key = ThumbnailKey::from_entry_at_size(entry, 192)
                .expect("PNG entry should be thumbnail eligible");
            let pixels = decode_thumbnail(&key).expect("thumbnail should decode");
            let (width, height, _, _, bytes) = pixels.into_parts();
            assert!(width <= 192 && height <= 192);
            assert!(!bytes.is_empty());
        }
    });

    let content = fixture.path().join("content-search");
    fs::create_dir(&content).expect("content-search directory should be created");
    let content_body = format!(
        "{}\nphase21a target marker\n",
        "bounded text ".repeat(2_500)
    );
    let benchmark_text = content_body.repeat(512);
    let baseline_started = Instant::now();
    let mut baseline_counts = (0_u64, 0_u64);
    for _ in 0..8 {
        baseline_counts = (
            benchmark_text.split_whitespace().count() as u64,
            benchmark_text.lines().count() as u64,
        );
        black_box(baseline_counts);
    }
    let baseline_elapsed = baseline_started.elapsed();
    let current_started = Instant::now();
    let mut current_counts = (0_u64, 0_u64);
    for _ in 0..8 {
        current_counts = count_text_facts(&benchmark_text);
        black_box(current_counts);
    }
    let current_elapsed = current_started.elapsed();
    assert_eq!(current_counts, baseline_counts);
    assert!(
        current_elapsed < baseline_elapsed,
        "one-pass text facts should beat the two-pass baseline: current={current_elapsed:?}, baseline={baseline_elapsed:?}"
    );
    println!(
        "PHASE21A_RESULT workload=metadata_text_facts baseline_us={} current_us={} bytes={} status=pass",
        baseline_elapsed.as_micros(),
        current_elapsed.as_micros(),
        benchmark_text.len()
    );
    for index in 0..512 {
        fs::write(
            content.join(format!("document-{index:04}.txt")),
            &content_body,
        )
        .expect("content-search fixture should be written");
    }
    let content_request = ContentSearchRequest::new(
        content.clone(),
        "target marker".to_owned(),
        FilenameSearchScope::CurrentFolder,
        false,
        FolderFilterMode::Text,
        floe_core::AdvancedFilter::default(),
    )
    .expect("content-search request should be valid");
    let mut content_matches = 0_usize;
    let content_summary = measure("content_search_512", Duration::from_secs(15), || {
        floe_core::search_contents_with_mime(
            &content_request,
            ContentSearchLimits::default(),
            || false,
            |_| None,
            |batch, _| {
                content_matches += batch.len();
                true
            },
        )
        .expect("content search should complete")
    });
    assert_eq!(content_matches, 512);
    assert_eq!(content_summary.examined_files, 512);

    let transfer = fixture.path().join("transfer");
    fs::create_dir(&transfer).expect("transfer directory should be created");
    let source = transfer.join("source.bin");
    write_repeated(&source, 32 * 1024 * 1024, 0x5a);
    let destination = transfer.join("destination.bin");
    let copy_request = CopyRequest::new(
        source.clone(),
        destination.clone(),
        ConflictPolicy::FailIfExists,
        SymlinkPolicy::Preserve,
    );
    let copy_outcome = measure("copy_32mib", Duration::from_secs(20), || {
        execute_copy(&copy_request, &CopyCancellation::new(), |_| {})
            .expect("bounded copy should complete")
    });
    assert_eq!(copy_outcome.bytes_copied(), 32 * 1024 * 1024);

    let checksum_request =
        ChecksumRequest::new(vec![destination.clone()], ChecksumAlgorithm::Sha256, None)
            .expect("checksum request should be valid");
    let checksum = measure("checksum_32mib", Duration::from_secs(20), || {
        execute_checksum(&checksum_request, || false, |_, _| {}).expect("checksum should complete")
    });
    assert_eq!(checksum.items[0].bytes, 32 * 1024 * 1024);

    let duplicates = fixture.path().join("duplicates");
    fs::create_dir(&duplicates).expect("duplicate directory should be created");
    for pair in 0..64_u8 {
        for copy in 0..2 {
            write_repeated(
                &duplicates.join(format!("pair-{pair:02}-{copy}.bin")),
                64 * 1024,
                pair,
            );
        }
    }
    let duplicate_request = DuplicateScanRequest::for_folder(duplicates.clone())
        .expect("duplicate request should be valid");
    let duplicate_outcome = measure("duplicate_scan_128", Duration::from_secs(30), || {
        find_duplicates(
            &duplicate_request,
            DuplicateScanLimits::default(),
            || false,
            |path| digest_bytes(path).map(DuplicateHashResult::computed),
            |_| {},
        )
        .expect("duplicate scan should complete")
    });
    assert_eq!(duplicate_outcome.groups().len(), 64);

    let fingerprint = measure("integrity_save_32mib", Duration::from_secs(20), || {
        save_fingerprint(destination.clone(), || false, |_, _| {})
            .expect("fingerprint should be saved")
    });
    let verification = measure("integrity_verify_32mib", Duration::from_secs(20), || {
        verify_fingerprint(&fingerprint, || false, |_, _| {}).expect("fingerprint should verify")
    });
    assert_eq!(verification, FingerprintVerification::Match);

    let metadata_entries = enumerate_directory(&content)
        .expect("metadata fixtures should enumerate")
        .into_entries()
        .into_iter()
        .map(Arc::new)
        .collect();
    let indexed = measure("advanced_metadata_512", Duration::from_secs(15), || {
        index_and_sort_for_performance(
            metadata_entries,
            DirectorySort::new(SortColumn::DocumentWordCount, SortDirection::Descending),
        )
        .expect("advanced metadata scan should complete")
    });
    assert_eq!(indexed.len(), 512);
    assert!(indexed.iter().all(|entry| {
        entry
            .indexed_sort_metadata()
            .and_then(|metadata| metadata.word_count)
            .is_some()
    }));

    let peak = peak_rss_kib().unwrap_or(0);
    println!(
        "PHASE21A_RESULT workload=peak_memory peak_rss_kib={peak} procedure=proc_status_vmhwm status=pass"
    );
    println!(
        "PHASE21A_RESULT workload=complete entries={} temporary_root=true status=pass",
        HUGE_ENTRY_COUNT
    );
}
