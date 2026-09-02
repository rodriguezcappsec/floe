//! Explainable filename/type risk signals, never a malware verdict.

use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspiciousSeverity {
    Information,
    Caution,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuspiciousFindingKind {
    Executable,
    ExecutableDocumentName,
    DoubleExtension {
        visible: String,
        actual: String,
    },
    MimeMismatch {
        extension: String,
        content_type: String,
    },
    DesktopLauncher,
    Script,
    AppImage,
    BidiControl,
    InvisibleOrControl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuspiciousFinding {
    pub kind: SuspiciousFindingKind,
    pub severity: SuspiciousSeverity,
    pub explanation: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuspiciousAnalysis {
    pub escaped_name: String,
    pub findings: Vec<SuspiciousFinding>,
}

pub fn analyze_suspicious_file(
    path: &Path,
    content_type: Option<&str>,
    executable: bool,
) -> SuspiciousAnalysis {
    let name = path.file_name().unwrap_or(path.as_os_str());
    let escaped_name = escaped_os_name(name);
    let mut findings = Vec::new();
    if executable {
        findings.push(finding(
            SuspiciousFindingKind::Executable,
            SuspiciousSeverity::Caution,
            "This file has executable permission and can run code when launched.",
        ));
    }

    if let Some(name) = name.to_str() {
        let lower = name.to_ascii_lowercase();
        let extensions = lower.split('.').skip(1).collect::<Vec<_>>();
        let final_extension = extensions.last().copied().unwrap_or_default();
        if extensions.len() >= 2 {
            let visible = extensions[extensions.len() - 2];
            if is_document_extension(visible) && is_active_extension(final_extension) {
                findings.push(finding(
                    SuspiciousFindingKind::DoubleExtension {
                        visible: visible.to_owned(),
                        actual: final_extension.to_owned(),
                    },
                    SuspiciousSeverity::High,
                    "The filename ends in an active type after a document-like extension.",
                ));
            }
        }
        match final_extension {
            "desktop" => findings.push(finding(
                SuspiciousFindingKind::DesktopLauncher,
                SuspiciousSeverity::High,
                "Desktop launcher files can execute commands rather than open passive content.",
            )),
            "sh" | "bash" | "zsh" | "fish" | "py" | "pl" | "rb" | "ps1" => {
                findings.push(finding(
                    SuspiciousFindingKind::Script,
                    SuspiciousSeverity::Caution,
                    "This filename identifies a script that may execute commands.",
                ));
            }
            "appimage" => findings.push(finding(
                SuspiciousFindingKind::AppImage,
                SuspiciousSeverity::Caution,
                "AppImages are executable application bundles and should come from a trusted source.",
            )),
            _ => {}
        }
        if executable && is_document_extension(final_extension) {
            findings.push(finding(
                SuspiciousFindingKind::ExecutableDocumentName,
                SuspiciousSeverity::High,
                "A document-looking filename unexpectedly has executable permission.",
            ));
        }
        if name.chars().any(is_bidi_control) {
            findings.push(finding(
                SuspiciousFindingKind::BidiControl,
                SuspiciousSeverity::High,
                "The filename contains bidirectional controls that can disguise character order.",
            ));
        }
        if name.chars().any(is_invisible_or_control) {
            findings.push(finding(
                SuspiciousFindingKind::InvisibleOrControl,
                SuspiciousSeverity::Caution,
                "The filename contains invisible or control characters; inspect the escaped name.",
            ));
        }
        if let Some(content_type) = content_type {
            if let Some(expected_prefix) = expected_mime_prefix(final_extension) {
                if !content_type.starts_with(expected_prefix) {
                    findings.push(finding(
                        SuspiciousFindingKind::MimeMismatch {
                            extension: final_extension.to_owned(),
                            content_type: content_type.to_owned(),
                        },
                        SuspiciousSeverity::Caution,
                        "The detected content type does not match the filename extension.",
                    ));
                }
            }
        }
    }
    SuspiciousAnalysis {
        escaped_name,
        findings,
    }
}

fn finding(
    kind: SuspiciousFindingKind,
    severity: SuspiciousSeverity,
    explanation: &'static str,
) -> SuspiciousFinding {
    SuspiciousFinding {
        kind,
        severity,
        explanation,
    }
}

fn is_document_extension(extension: &str) -> bool {
    matches!(
        extension,
        "pdf"
            | "txt"
            | "doc"
            | "docx"
            | "odt"
            | "xls"
            | "xlsx"
            | "ods"
            | "ppt"
            | "pptx"
            | "odp"
            | "jpg"
            | "jpeg"
            | "png"
            | "gif"
    )
}

fn is_active_extension(extension: &str) -> bool {
    matches!(
        extension,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "desktop"
            | "appimage"
            | "run"
            | "bin"
            | "exe"
            | "com"
            | "scr"
            | "bat"
            | "cmd"
            | "ps1"
            | "jar"
    )
}

fn expected_mime_prefix(extension: &str) -> Option<&'static str> {
    match extension {
        "pdf" => Some("application/pdf"),
        "txt" | "md" | "csv" | "log" => Some("text/"),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" => Some("image/"),
        "mp3" | "flac" | "wav" | "ogg" | "m4a" => Some("audio/"),
        "mp4" | "mkv" | "webm" | "avi" | "mov" => Some("video/"),
        "zip" | "7z" | "gz" | "xz" | "bz2" | "tar" => Some("application/"),
        _ => None,
    }
}

fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

fn is_invisible_or_control(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{00ad}' | '\u{034f}' | '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}'
        )
}

pub fn escaped_os_name(name: &OsStr) -> String {
    let mut escaped = String::new();
    for byte in name.as_bytes() {
        match byte {
            b' '..=b'~' if *byte != b'\\' => escaped.push(char::from(*byte)),
            b'\\' => escaped.push_str("\\\\"),
            _ => escaped.push_str(&format!("\\x{byte:02x}")),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    use super::*;

    #[test]
    fn phase_18n_suspicious_finds_double_extension_execution_and_mismatch() {
        let analysis = analyze_suspicious_file(
            Path::new("invoice.pdf.sh"),
            Some("application/x-shellscript"),
            true,
        );
        assert!(
            analysis.findings.iter().any(|finding| matches!(
                finding.kind,
                SuspiciousFindingKind::DoubleExtension { .. }
            ))
        );
        assert!(
            analysis
                .findings
                .iter()
                .any(|finding| finding.kind == SuspiciousFindingKind::Script)
        );

        let mismatch = analyze_suspicious_file(
            Path::new("photo.png"),
            Some("application/x-executable"),
            true,
        );
        assert!(
            mismatch
                .findings
                .iter()
                .any(|finding| matches!(finding.kind, SuspiciousFindingKind::MimeMismatch { .. }))
        );
        assert!(
            mismatch
                .findings
                .iter()
                .any(|finding| finding.kind == SuspiciousFindingKind::ExecutableDocumentName)
        );
    }

    #[test]
    fn phase_18n_suspicious_explains_unicode_controls_and_raw_names() {
        let analysis = analyze_suspicious_file(
            Path::new("report\u{202e}fdp.exe"),
            Some("application/x-executable"),
            false,
        );
        assert!(
            analysis
                .findings
                .iter()
                .any(|finding| finding.kind == SuspiciousFindingKind::BidiControl)
        );
        let raw = PathBuf::from(OsString::from_vec(b"raw-\xff.txt".to_vec()));
        let raw_analysis = analyze_suspicious_file(&raw, Some("text/plain"), false);
        assert_eq!(raw_analysis.escaped_name, "raw-\\xff.txt");
        assert!(raw_analysis.findings.is_empty());
    }

    #[test]
    fn phase_18n_suspicious_avoids_common_compound_extension_false_positive() {
        for name in ["archive.tar.gz", "notes.txt", "photo.jpeg", "module.min.js"] {
            let analysis = analyze_suspicious_file(Path::new(name), None, false);
            assert!(!analysis.findings.iter().any(|finding| matches!(
                finding.kind,
                SuspiciousFindingKind::DoubleExtension { .. }
            )));
        }
    }
}
