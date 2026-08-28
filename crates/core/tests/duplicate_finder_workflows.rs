use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use floe_core::{
    DuplicateHashError, DuplicateHashResult, DuplicateScanLimits, DuplicateScanRequest,
    find_duplicates,
};
use tempfile::tempdir;

fn test_digest(path: &Path) -> Result<DuplicateHashResult, DuplicateHashError> {
    let bytes = fs::read(path).map_err(|error| DuplicateHashError::Failed(error.to_string()))?;
    let mut digest = [0_u8; 32];
    for (index, byte) in bytes.iter().enumerate() {
        digest[index % digest.len()] ^= *byte;
    }
    Ok(DuplicateHashResult::computed(digest))
}

#[test]
fn folder_tree_mode_finds_exact_duplicates_across_nested_subfolders() {
    let fixture = tempdir().expect("fixture");
    let left = fixture.path().join("first/inside/video-one.mkv");
    let right = fixture.path().join("second/deeper/video-renamed.mkv");
    fs::create_dir_all(left.parent().expect("left parent")).expect("left tree");
    fs::create_dir_all(right.parent().expect("right parent")).expect("right tree");
    fs::write(&left, b"the same video bytes").expect("left file");
    fs::write(&right, b"the same video bytes").expect("right file");

    let outcome = find_duplicates(
        &DuplicateScanRequest::for_folder(fixture.path().to_path_buf()).expect("request"),
        DuplicateScanLimits::default(),
        || false,
        test_digest,
        |_| {},
    )
    .expect("scan");

    assert_eq!(outcome.groups().len(), 1);
    let paths = outcome.groups()[0]
        .items()
        .iter()
        .map(|item| item.path())
        .collect::<Vec<_>>();
    assert!(paths.contains(&left.as_path()));
    assert!(paths.contains(&right.as_path()));
}

#[test]
fn reference_mode_finds_nested_copies_and_excludes_unrelated_groups() {
    let fixture = tempdir().expect("fixture");
    let reference = fixture.path().join("reference.bin");
    let copy = fixture.path().join("nested/far/copy.bin");
    let unrelated_one = fixture.path().join("other/one.bin");
    let unrelated_two = fixture.path().join("other/deeper/two.bin");
    fs::create_dir_all(copy.parent().expect("copy parent")).expect("copy tree");
    fs::create_dir_all(unrelated_two.parent().expect("other parent")).expect("other tree");
    fs::write(&reference, b"reference bytes").expect("reference");
    fs::write(&copy, b"reference bytes").expect("copy");
    fs::write(&unrelated_one, b"unrelated bytes").expect("unrelated one");
    fs::write(&unrelated_two, b"unrelated bytes").expect("unrelated two");

    let outcome = find_duplicates(
        &DuplicateScanRequest::for_reference(reference.clone(), fixture.path().to_path_buf())
            .expect("request"),
        DuplicateScanLimits::default(),
        || false,
        test_digest,
        |_| {},
    )
    .expect("scan");

    assert_eq!(outcome.groups().len(), 1);
    let group = &outcome.groups()[0];
    assert!(group.items().iter().any(|item| item.path() == reference));
    assert!(group.items().iter().any(|item| item.path() == copy));
    assert!(
        !group
            .items()
            .iter()
            .any(|item| item.path() == unrelated_one || item.path() == unrelated_two)
    );
}

#[test]
fn reference_inside_scope_is_not_counted_twice_and_preserves_raw_path_identity() {
    let fixture = tempdir().expect("fixture");
    let raw_name = OsString::from_vec(vec![b'r', b'e', b'f', 0xff]);
    let reference = fixture.path().join(raw_name);
    let copy = fixture.path().join("nested/copy");
    fs::create_dir_all(copy.parent().expect("copy parent")).expect("copy tree");
    fs::write(&reference, b"same raw bytes").expect("reference");
    fs::write(&copy, b"same raw bytes").expect("copy");

    let outcome = find_duplicates(
        &DuplicateScanRequest::for_reference(reference.clone(), fixture.path().to_path_buf())
            .expect("request"),
        DuplicateScanLimits::default(),
        || false,
        test_digest,
        |_| {},
    )
    .expect("scan");

    assert_eq!(outcome.groups().len(), 1);
    assert_eq!(
        outcome.groups()[0]
            .items()
            .iter()
            .filter(|item| item.path() == reference)
            .count(),
        1
    );
}

#[test]
fn reference_mode_rejects_a_non_regular_reference() {
    let fixture = tempdir().expect("fixture");
    let reference = fixture.path().join("folder-reference");
    fs::create_dir(&reference).expect("reference folder");

    let error = find_duplicates(
        &DuplicateScanRequest::for_reference(reference.clone(), fixture.path().to_path_buf())
            .expect("request"),
        DuplicateScanLimits::default(),
        || false,
        test_digest,
        |_| {},
    )
    .expect_err("directory cannot be a reference file");

    assert!(error.to_string().contains("reference file is unavailable"));
}

#[test]
fn phase_13g3_quick_signature_rejects_same_size_nonmatches_before_sha256() {
    let fixture = tempdir().expect("fixture");
    let mut duplicate = vec![b'a'; 256 * 1024];
    duplicate[128 * 1024] = b'b';
    let mut different = duplicate.clone();
    different[0] = b'z';
    fs::write(fixture.path().join("one"), &duplicate).expect("one");
    fs::write(fixture.path().join("two"), &duplicate).expect("two");
    fs::write(fixture.path().join("different"), &different).expect("different");
    let hash_calls = AtomicUsize::new(0);

    let outcome = find_duplicates(
        &DuplicateScanRequest::for_folder(fixture.path().to_path_buf()).expect("request"),
        DuplicateScanLimits::default(),
        || false,
        |path| {
            hash_calls.fetch_add(1, Ordering::SeqCst);
            test_digest(path)
        },
        |_| {},
    )
    .expect("scan");

    assert_eq!(hash_calls.load(Ordering::SeqCst), 2);
    assert_eq!(outcome.groups().len(), 1);
    assert_eq!(outcome.summary().quick_checked_files, 3);
    assert_eq!(outcome.summary().hashed_files, 2);
}

#[test]
fn phase_13g3_parallel_hash_is_bounded_and_uses_device_aware_concurrency() {
    let fixture = tempdir().expect("fixture");
    for index in 0..8 {
        fs::write(
            fixture.path().join(format!("copy-{index}")),
            vec![7_u8; 256 * 1024],
        )
        .expect("copy");
    }
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let active_for_hash = Arc::clone(&active);
    let maximum_for_hash = Arc::clone(&maximum);

    let outcome = find_duplicates(
        &DuplicateScanRequest::for_folder(fixture.path().to_path_buf()).expect("request"),
        DuplicateScanLimits {
            hash_workers: 8,
            ..DuplicateScanLimits::default()
        },
        || false,
        move |path| {
            let now = active_for_hash.fetch_add(1, Ordering::SeqCst) + 1;
            maximum_for_hash.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
            let result = test_digest(path);
            active_for_hash.fetch_sub(1, Ordering::SeqCst);
            result
        },
        |_| {},
    )
    .expect("scan");

    assert_eq!(outcome.groups().len(), 1);
    assert_eq!(outcome.summary().hashed_files, 8);
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
}
