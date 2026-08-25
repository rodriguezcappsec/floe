use std::{collections::HashSet, path::PathBuf};

use gtk::{gdk, gio, glib, prelude::*};
use thiserror::Error;

use crate::state::TransferIntent;

pub const URI_LIST_MIME: &str = "text/uri-list";
pub const GNOME_COPIED_FILES_MIME: &str = "x-special/gnome-copied-files";
pub const KDE_CUT_SELECTION_MIME: &str = "application/x-kde-cutselection";
const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;
const MAX_CLIPBOARD_ITEMS: usize = 4096;
const READ_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardTransfer {
    intent: TransferIntent,
    paths: Vec<PathBuf>,
}

impl ClipboardTransfer {
    pub fn new(
        intent: TransferIntent,
        paths: Vec<PathBuf>,
    ) -> Result<Self, ClipboardTransferError> {
        if paths.is_empty() {
            return Err(ClipboardTransferError::Empty);
        }
        if paths.len() > MAX_CLIPBOARD_ITEMS {
            return Err(ClipboardTransferError::TooManyItems);
        }
        let mut unique = Vec::with_capacity(paths.len());
        let mut seen = HashSet::with_capacity(paths.len());
        for path in paths {
            if !path.is_absolute() {
                return Err(ClipboardTransferError::RelativePath(path));
            }
            if seen.insert(path.clone()) {
                unique.push(path);
            }
        }
        if unique.is_empty() {
            return Err(ClipboardTransferError::Empty);
        }
        Ok(Self {
            intent,
            paths: unique,
        })
    }

    pub const fn intent(&self) -> TransferIntent {
        self.intent
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedClipboardTransfer {
    uri_list: Vec<u8>,
    gnome_copied_files: Vec<u8>,
    kde_cut_selection: Vec<u8>,
}

impl EncodedClipboardTransfer {
    pub fn uri_list(&self) -> &[u8] {
        &self.uri_list
    }

    pub fn gnome_copied_files(&self) -> &[u8] {
        &self.gnome_copied_files
    }

    pub fn kde_cut_selection(&self) -> &[u8] {
        &self.kde_cut_selection
    }
}

#[derive(Debug, Error)]
pub enum ClipboardTransferError {
    #[error("clipboard transfer contains no local files")]
    Empty,
    #[error("clipboard transfer exceeds the {MAX_CLIPBOARD_ITEMS}-item safety limit")]
    TooManyItems,
    #[error("clipboard data exceeds the {MAX_CLIPBOARD_BYTES}-byte safety limit")]
    TooLarge,
    #[error("clipboard path is not absolute: {}", .0.display())]
    RelativePath(PathBuf),
    #[error("clipboard URI is not valid UTF-8")]
    InvalidUtf8,
    #[error("clipboard URI is not a local file URI: {0}")]
    NonLocalUri(String),
    #[error("clipboard data has an invalid copy/cut marker")]
    InvalidIntent,
    #[error("could not convert local path to a file URI: {0}")]
    EncodeUri(String),
    #[error("could not read desktop clipboard: {0}")]
    Read(String),
    #[error("could not publish desktop clipboard: {0}")]
    Publish(String),
}

pub fn encode_transfer(
    transfer: &ClipboardTransfer,
) -> Result<EncodedClipboardTransfer, ClipboardTransferError> {
    let mut uri_list = Vec::new();
    for path in transfer.paths() {
        let uri = glib::filename_to_uri(path, None)
            .map_err(|error| ClipboardTransferError::EncodeUri(error.to_string()))?;
        if !uri_list.is_empty() {
            uri_list.extend_from_slice(b"\r\n");
        }
        uri_list.extend_from_slice(uri.as_bytes());
        if uri_list.len() > MAX_CLIPBOARD_BYTES {
            return Err(ClipboardTransferError::TooLarge);
        }
    }
    uri_list.extend_from_slice(b"\r\n");

    let marker = match transfer.intent() {
        TransferIntent::Copy => b"copy\n".as_slice(),
        TransferIntent::Move => b"cut\n".as_slice(),
    };
    let mut gnome_copied_files = Vec::with_capacity(marker.len() + uri_list.len());
    gnome_copied_files.extend_from_slice(marker);
    gnome_copied_files.extend_from_slice(&uri_list);
    if gnome_copied_files.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardTransferError::TooLarge);
    }

    Ok(EncodedClipboardTransfer {
        uri_list,
        gnome_copied_files,
        kde_cut_selection: match transfer.intent() {
            TransferIntent::Copy => b"0".to_vec(),
            TransferIntent::Move => b"1".to_vec(),
        },
    })
}

pub fn publish_transfer(
    clipboard: &gdk::Clipboard,
    transfer: &ClipboardTransfer,
) -> Result<(), ClipboardTransferError> {
    let encoded = encode_transfer(transfer)?;
    let providers = [
        gdk::ContentProvider::for_bytes(URI_LIST_MIME, &glib::Bytes::from(encoded.uri_list())),
        gdk::ContentProvider::for_bytes(
            GNOME_COPIED_FILES_MIME,
            &glib::Bytes::from(encoded.gnome_copied_files()),
        ),
        gdk::ContentProvider::for_bytes(
            KDE_CUT_SELECTION_MIME,
            &glib::Bytes::from(encoded.kde_cut_selection()),
        ),
    ];
    clipboard
        .set_content(Some(&gdk::ContentProvider::new_union(&providers)))
        .map_err(|error| ClipboardTransferError::Publish(error.to_string()))
}

pub fn contains_transfer(clipboard: &gdk::Clipboard) -> bool {
    let formats = clipboard.formats();
    formats.contain_mime_type(GNOME_COPIED_FILES_MIME) || formats.contain_mime_type(URI_LIST_MIME)
}

pub fn read_transfer_async<F>(clipboard: &gdk::Clipboard, callback: F)
where
    F: FnOnce(Result<ClipboardTransfer, ClipboardTransferError>) + 'static,
{
    let formats = clipboard.formats();
    if formats.contain_mime_type(GNOME_COPIED_FILES_MIME) {
        read_mime_bytes(
            clipboard,
            GNOME_COPIED_FILES_MIME,
            Box::new(move |result| {
                callback(result.and_then(|bytes| parse_gnome_copied_files(&bytes)))
            }),
        );
        return;
    }
    if !formats.contain_mime_type(URI_LIST_MIME) {
        callback(Err(ClipboardTransferError::Empty));
        return;
    }

    let has_kde_marker = formats.contain_mime_type(KDE_CUT_SELECTION_MIME);
    let clipboard = clipboard.clone();
    read_mime_bytes(
        &clipboard.clone(),
        URI_LIST_MIME,
        Box::new(move |uri_result| {
            let uri_bytes = match uri_result {
                Ok(bytes) => bytes,
                Err(error) => {
                    callback(Err(error));
                    return;
                }
            };
            if !has_kde_marker {
                callback(parse_uri_list(TransferIntent::Copy, &uri_bytes));
                return;
            }
            read_mime_bytes(
                &clipboard,
                KDE_CUT_SELECTION_MIME,
                Box::new(move |marker_result| {
                    let result = marker_result
                        .and_then(|marker| parse_kde_intent(&marker))
                        .and_then(|intent| parse_uri_list(intent, &uri_bytes));
                    callback(result);
                }),
            );
        }),
    );
}

type ReadCallback = Box<dyn FnOnce(Result<Vec<u8>, ClipboardTransferError>)>;

fn read_mime_bytes(clipboard: &gdk::Clipboard, mime: &'static str, callback: ReadCallback) {
    clipboard.read_async(
        &[mime],
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |result| match result {
            Ok((stream, _)) => read_stream_chunk(stream, Vec::new(), callback),
            Err(error) => callback(Err(ClipboardTransferError::Read(error.to_string()))),
        },
    );
}

fn read_stream_chunk(stream: gio::InputStream, bytes: Vec<u8>, callback: ReadCallback) {
    stream.clone().read_bytes_async(
        READ_CHUNK_BYTES,
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(chunk) if chunk.is_empty() => callback(Ok(bytes)),
            Ok(chunk) => {
                if bytes.len().saturating_add(chunk.len()) > MAX_CLIPBOARD_BYTES {
                    callback(Err(ClipboardTransferError::TooLarge));
                    return;
                }
                let mut bytes = bytes;
                bytes.extend_from_slice(chunk.as_ref());
                read_stream_chunk(stream, bytes, callback);
            }
            Err(error) => callback(Err(ClipboardTransferError::Read(error.to_string()))),
        },
    );
}

fn parse_gnome_copied_files(bytes: &[u8]) -> Result<ClipboardTransfer, ClipboardTransferError> {
    check_byte_limit(bytes)?;
    let Some((marker, uris)) = split_first_line(bytes) else {
        return Err(ClipboardTransferError::InvalidIntent);
    };
    let intent = match trim_line(marker) {
        b"copy" => TransferIntent::Copy,
        b"cut" => TransferIntent::Move,
        _ => return Err(ClipboardTransferError::InvalidIntent),
    };
    parse_uri_list(intent, uris)
}

fn parse_kde_intent(bytes: &[u8]) -> Result<TransferIntent, ClipboardTransferError> {
    check_byte_limit(bytes)?;
    match trim_ascii_whitespace(bytes) {
        b"0" => Ok(TransferIntent::Copy),
        b"1" => Ok(TransferIntent::Move),
        _ => Err(ClipboardTransferError::InvalidIntent),
    }
}

fn parse_uri_list(
    intent: TransferIntent,
    bytes: &[u8],
) -> Result<ClipboardTransfer, ClipboardTransferError> {
    check_byte_limit(bytes)?;
    let mut paths = Vec::new();
    for line in bytes.split(|byte| *byte == b'\n') {
        let line = trim_line(line);
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        if paths.len() >= MAX_CLIPBOARD_ITEMS {
            return Err(ClipboardTransferError::TooManyItems);
        }
        let uri = std::str::from_utf8(line).map_err(|_| ClipboardTransferError::InvalidUtf8)?;
        let (path, hostname) = glib::filename_from_uri(uri)
            .map_err(|_| ClipboardTransferError::NonLocalUri(uri.to_owned()))?;
        if hostname.as_deref().is_some_and(|hostname| {
            !hostname.is_empty() && !hostname.eq_ignore_ascii_case("localhost")
        }) || !path.is_absolute()
        {
            return Err(ClipboardTransferError::NonLocalUri(uri.to_owned()));
        }
        paths.push(path);
    }
    ClipboardTransfer::new(intent, paths)
}

fn check_byte_limit(bytes: &[u8]) -> Result<(), ClipboardTransferError> {
    if bytes.len() > MAX_CLIPBOARD_BYTES {
        Err(ClipboardTransferError::TooLarge)
    } else {
        Ok(())
    }
}

fn split_first_line(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|byte| *byte == b'\n')?;
    Some((&bytes[..index], &bytes[index + 1..]))
}

fn trim_line(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn trim_ascii_whitespace(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::Path,
    };

    use super::*;

    #[test]
    fn phase_6o_clipboard_publish_round_trips_copy_cut_and_non_utf8_paths() {
        let paths = vec![
            PathBuf::from("/tmp/space and # marker"),
            Path::new("/tmp").join(OsString::from_vec(b"raw-\xff".to_vec())),
        ];
        for intent in [TransferIntent::Copy, TransferIntent::Move] {
            let transfer = ClipboardTransfer::new(intent, paths.clone())
                .expect("exact local paths should be accepted");
            let encoded = encode_transfer(&transfer).expect("clipboard should encode");
            let decoded = parse_gnome_copied_files(encoded.gnome_copied_files())
                .expect("GNOME clipboard should round trip");
            assert_eq!(decoded, transfer);
            assert_eq!(
                parse_kde_intent(encoded.kde_cut_selection()).expect("KDE marker should decode"),
                intent
            );
            assert!(encoded.uri_list().windows(3).any(|part| part == b"%FF"));
        }
    }

    #[test]
    fn phase_6o_clipboard_publish_deduplicates_exact_paths_and_rejects_relative() {
        let path = PathBuf::from("/tmp/item");
        let transfer =
            ClipboardTransfer::new(TransferIntent::Copy, vec![path.clone(), path.clone()])
                .expect("duplicate local paths should be accepted once");
        assert_eq!(transfer.paths(), &[path]);
        assert!(matches!(
            ClipboardTransfer::new(TransferIntent::Copy, vec![PathBuf::from("relative")]),
            Err(ClipboardTransferError::RelativePath(_))
        ));

        let too_many_paths = (0..=MAX_CLIPBOARD_ITEMS)
            .map(|index| PathBuf::from(format!("/tmp/item-{index}")))
            .collect();
        assert!(matches!(
            ClipboardTransfer::new(TransferIntent::Copy, too_many_paths),
            Err(ClipboardTransferError::TooManyItems)
        ));

        let oversized_path = Path::new("/tmp").join("x".repeat(MAX_CLIPBOARD_BYTES));
        let oversized = ClipboardTransfer::new(TransferIntent::Copy, vec![oversized_path])
            .expect("one absolute path remains within the item limit");
        assert!(matches!(
            encode_transfer(&oversized),
            Err(ClipboardTransferError::TooLarge)
        ));
    }

    #[test]
    fn phase_6o_clipboard_parse_supports_gnome_kde_and_uri_list_semantics() {
        let uris = b"# comment\r\nfile:///tmp/one\r\nfile:///tmp/two\r\n";
        let copy = parse_uri_list(TransferIntent::Copy, uris).expect("URI list should parse");
        assert_eq!(copy.intent(), TransferIntent::Copy);
        assert_eq!(
            copy.paths(),
            &[PathBuf::from("/tmp/one"), PathBuf::from("/tmp/two")]
        );

        let cut =
            parse_gnome_copied_files(b"cut\nfile:///tmp/one\nfile:///tmp/one\nfile:///tmp/two\n")
                .expect("GNOME cut list should parse");
        assert_eq!(cut.intent(), TransferIntent::Move);
        assert_eq!(cut.paths().len(), 2);
        assert_eq!(
            parse_kde_intent(b" 1\n").expect("KDE cut marker should parse"),
            TransferIntent::Move
        );
    }

    #[test]
    fn phase_6o_clipboard_parse_rejects_remote_malformed_and_oversized_data() {
        assert!(matches!(
            parse_uri_list(TransferIntent::Copy, b"https://example.com/file\n"),
            Err(ClipboardTransferError::NonLocalUri(_))
        ));
        assert!(matches!(
            parse_uri_list(TransferIntent::Copy, b"file://remote-host/tmp/file\n"),
            Err(ClipboardTransferError::NonLocalUri(_))
        ));
        assert!(matches!(
            parse_gnome_copied_files(b"move\nfile:///tmp/file\n"),
            Err(ClipboardTransferError::InvalidIntent)
        ));
        assert!(matches!(
            parse_uri_list(TransferIntent::Copy, &vec![b'a'; MAX_CLIPBOARD_BYTES + 1]),
            Err(ClipboardTransferError::TooLarge)
        ));

        let over_item_limit = (0..=MAX_CLIPBOARD_ITEMS)
            .map(|index| format!("file:///tmp/item-{index}\n"))
            .collect::<String>();
        assert!(matches!(
            parse_uri_list(TransferIntent::Copy, over_item_limit.as_bytes()),
            Err(ClipboardTransferError::TooManyItems)
        ));
    }

    #[test]
    fn phase_6o_clipboard_parse_preserves_colliding_lossy_names_by_raw_path() {
        let first = Path::new("/tmp").join(OsString::from_vec(b"name-\xfe".to_vec()));
        let second = Path::new("/tmp").join(OsString::from_vec(b"name-\xff".to_vec()));
        let transfer = ClipboardTransfer::new(TransferIntent::Copy, vec![first, second])
            .expect("raw paths should remain distinct");
        let encoded = encode_transfer(&transfer).expect("raw paths should encode");
        let parsed = parse_uri_list(TransferIntent::Copy, encoded.uri_list())
            .expect("raw paths should decode");
        assert_eq!(parsed.paths().len(), 2);
        assert_ne!(
            parsed.paths()[0].as_os_str().as_bytes(),
            parsed.paths()[1].as_os_str().as_bytes()
        );
    }
}
