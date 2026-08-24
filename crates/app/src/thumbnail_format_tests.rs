use std::{
    fs::{self, File},
    path::Path,
    thread,
    time::{Duration, SystemTime},
};

use floe_core::{DirectoryEntry, enumerate_directory};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tempfile::tempdir;

use super::*;

fn entry_for<'a>(entries: &'a [DirectoryEntry], path: &Path) -> &'a DirectoryEntry {
    entries
        .iter()
        .find(|entry| entry.path() == path)
        .expect("test entry should be enumerated")
}

fn wait_for_response(worker: &ThumbnailWorker) -> ThumbnailResponse {
    (0..200)
        .find_map(|_| {
            let response = worker.try_response();
            if response.is_none() {
                thread::sleep(Duration::from_millis(5));
            }
            response
        })
        .expect("thumbnail response should arrive")
}

fn write_test_image(path: &Path, format: ImageFormat, width: u32, height: u32) {
    let image = RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            u8::try_from(x % 251).expect("bounded red channel"),
            u8::try_from(y % 251).expect("bounded green channel"),
            97,
            255,
        ])
    });
    DynamicImage::ImageRgba8(image)
        .save_with_format(path, format)
        .expect("test image should encode");
}

fn add_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    assert!(jpeg.starts_with(&[0xff, 0xd8]));
    let mut exif = vec![
        0xff, 0xe1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0x00, 0x00, b'M', b'M', 0x00, 0x2a, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x01, 0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01,
    ];
    exif.extend_from_slice(&orientation.to_be_bytes());
    exif.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let mut oriented = Vec::with_capacity(jpeg.len() + exif.len());
    oriented.extend_from_slice(&jpeg[..2]);
    oriented.append(&mut exif);
    oriented.extend_from_slice(&jpeg[2..]);
    oriented
}

#[test]
fn phase_6f_format_policy_allows_reviewed_static_formats_case_insensitively() {
    let accepted = [
        ("image.png", ThumbnailFormat::Png),
        ("image.JPEG", ThumbnailFormat::Jpeg),
        ("image.WeBp", ThumbnailFormat::WebP),
        ("image.GIF", ThumbnailFormat::Gif),
        ("image.bmp", ThumbnailFormat::Bmp),
        ("image.TIF", ThumbnailFormat::Tiff),
        ("image.tiff", ThumbnailFormat::Tiff),
        ("image.ICO", ThumbnailFormat::Ico),
    ];
    for (name, expected) in accepted {
        assert_eq!(ThumbnailFormat::from_path(Path::new(name)), Some(expected));
    }
    for rejected in [
        "image.svg",
        "image.svgz",
        "image.avif",
        "image.heic",
        "image.qoi",
    ] {
        assert_eq!(ThumbnailFormat::from_path(Path::new(rejected)), None);
    }
}

#[test]
fn phase_6f_each_added_static_decoder_returns_owned_pixels() {
    let directory = tempdir().expect("temporary directory should be created");
    let formats = [
        ("image.webp", ImageFormat::WebP),
        ("image.gif", ImageFormat::Gif),
        ("image.bmp", ImageFormat::Bmp),
        ("image.tiff", ImageFormat::Tiff),
        ("image.ico", ImageFormat::Ico),
    ];
    for (name, format) in formats {
        write_test_image(&directory.path().join(name), format, 16, 16);
    }
    let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
    for (name, _) in formats {
        let path = directory.path().join(name);
        let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
            .expect("reviewed format should be eligible");
        let pixels = decode_thumbnail(&key).expect("reviewed format should decode");
        assert_eq!((pixels.width, pixels.height), (16, 16));
        assert!(pixels.has_alpha);
        assert_eq!(pixels.rowstride, 64);
    }
}

#[test]
fn phase_6f_orientation_is_applied_before_scaling_and_cache_storage() {
    let directory = tempdir().expect("temporary directory should be created");
    let path = directory.path().join("oriented.jpg");
    write_test_image(&path, ImageFormat::Jpeg, 64, 16);
    let jpeg = fs::read(&path).expect("JPEG should be readable");
    fs::write(&path, add_exif_orientation(&jpeg, 6)).expect("oriented JPEG should be written");
    let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
    let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
        .expect("oriented JPEG should be eligible");
    let mut cache = ThumbnailCache::new(ThumbnailCacheConfig::for_test(
        directory.path().join("cache"),
    ));
    cache.initialize().expect("cache should initialize");
    let pixels =
        decode_thumbnail_with_cache(&key, Some(&mut cache)).expect("oriented JPEG should decode");
    assert_eq!((pixels.width, pixels.height), (8, 32));
    let cached = cache
        .load(&key)
        .expect("cache lookup should succeed")
        .expect("oriented thumbnail should be cached");
    assert_eq!(cached.dimensions(), (16, 64));
}

#[test]
fn phase_6f_webp_scaling_preserves_aspect_ratio_and_malformed_input_fails() {
    let directory = tempdir().expect("temporary directory should be created");
    let wide = directory.path().join("wide.webp");
    let malformed = directory.path().join("malformed.webp");
    write_test_image(&wide, ImageFormat::WebP, 64, 16);
    fs::write(&malformed, b"not a WebP image").expect("malformed input should be written");
    let listing = enumerate_directory(directory.path()).expect("directory should enumerate");

    let wide_key = ThumbnailKey::from_entry(entry_for(listing.entries(), &wide))
        .expect("WebP should be eligible");
    let pixels = decode_thumbnail(&wide_key).expect("WebP should decode");
    assert_eq!((pixels.width, pixels.height), (32, 8));

    let malformed_key = ThumbnailKey::from_entry(entry_for(listing.entries(), &malformed))
        .expect("malformed WebP remains name-eligible");
    assert!(matches!(
        decode_thumbnail(&malformed_key),
        Err(ThumbnailError::Decode(_))
    ));
}

#[test]
fn phase_6f_added_format_persistent_cache_reuses_oriented_worker_result() {
    let directory = tempdir().expect("temporary directory should be created");
    let path = directory.path().join("image.webp");
    write_test_image(&path, ImageFormat::WebP, 64, 16);
    let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
    let key = ThumbnailKey::from_entry(entry_for(listing.entries(), &path))
        .expect("WebP should be eligible");
    let cache_home = directory.path().join("cache");
    let config = ThumbnailCacheConfig::for_test(cache_home);

    let mut first_worker = ThumbnailWorker::spawn_with_cache(1, None, Some(config.clone()))
        .expect("first worker should start");
    let first_generation = first_worker.begin_generation();
    first_worker
        .try_request(first_generation, key.clone())
        .expect("first request should enter queue");
    let first = wait_for_response(&first_worker);
    let first_pixels = first.result.expect("first WebP request should decode");
    assert_eq!((first_pixels.width, first_pixels.height), (32, 8));
    drop(first_worker);

    let original_time = key
        .modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("test source time should follow the epoch");
    fs::write(
        &path,
        vec![b'x'; usize::try_from(key.size).expect("source size should fit")],
    )
    .expect("source should become invalid without changing size");
    let source = File::open(&path).expect("source should reopen");
    let timestamp = rustix::fs::Timespec {
        tv_sec: i64::try_from(original_time.as_secs()).expect("test timestamp should fit"),
        tv_nsec: i64::from(original_time.subsec_nanos()),
    };
    rustix::fs::futimens(
        &source,
        &rustix::fs::Timestamps {
            last_access: timestamp,
            last_modification: timestamp,
        },
    )
    .expect("source modification time should be restored");

    let mut second_worker = ThumbnailWorker::spawn_with_cache(1, None, Some(config))
        .expect("second worker should start");
    let second_generation = second_worker.begin_generation();
    second_worker
        .try_request(second_generation, key)
        .expect("second request should enter queue");
    let second = wait_for_response(&second_worker);
    let second_pixels = second
        .result
        .expect("cached WebP should bypass invalid source pixels");
    assert_eq!((second_pixels.width, second_pixels.height), (32, 8));
}
