use std::{path::PathBuf, sync::Arc};

use floe_core::{ChecksumAlgorithm, ChecksumRequest, ChecksumRequestError, ExpectedDigest};
use thiserror::Error;

use crate::checksum_executor::{ChecksumOutcome, ChecksumVerification};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumDialogInput {
    pub algorithm: ChecksumAlgorithm,
    pub expected: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ChecksumDialogError {
    #[error("expected digest is invalid for the selected algorithm")]
    InvalidExpected,
    #[error(transparent)]
    Request(#[from] ChecksumRequestError),
}

pub fn build_checksum_request(
    targets: Arc<[PathBuf]>,
    input: &ChecksumDialogInput,
) -> Result<ChecksumRequest, ChecksumDialogError> {
    let expected = if input.expected.trim().is_empty() {
        None
    } else {
        Some(
            ExpectedDigest::parse(input.algorithm, input.expected.trim())
                .map_err(|_| ChecksumDialogError::InvalidExpected)?,
        )
    };
    ChecksumRequest::new(targets.to_vec(), input.algorithm, expected).map_err(Into::into)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumResultRow {
    pub display_name: String,
    pub digest: String,
    pub verification: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumPresentation {
    pub title: String,
    pub algorithm_label: &'static str,
    pub rows: Vec<ChecksumResultRow>,
    pub copy_text: String,
    pub notice: &'static str,
}

pub fn present_checksum(outcome: &ChecksumOutcome) -> ChecksumPresentation {
    let algorithm = outcome
        .items
        .first()
        .map_or(ChecksumAlgorithm::Sha256, |item| item.algorithm);
    let rows = outcome
        .items
        .iter()
        .map(|item| ChecksumResultRow {
            display_name: item
                .path
                .file_name()
                .unwrap_or(item.path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            digest: item.digest.clone(),
            verification: match &item.verification {
                ChecksumVerification::NotRequested => "Calculated; not compared".to_owned(),
                ChecksumVerification::Match => "Matches the supplied digest".to_owned(),
                ChecksumVerification::Mismatch { expected } => {
                    format!("Does not match supplied digest {expected}")
                }
            },
        })
        .collect::<Vec<_>>();
    let copy_text = outcome
        .items
        .iter()
        .map(|item| item.digest.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    ChecksumPresentation {
        title: format!(
            "{} checksum{}",
            outcome.items.len(),
            if outcome.items.len() == 1 { "" } else { "s" }
        ),
        algorithm_label: algorithm.display_name(),
        rows,
        copy_text,
        notice: "A checksum compares bytes. It does not prove authenticity, authorship, freshness, or safety.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checksum_executor::{ChecksumItemResult, ChecksumVerification};

    #[test]
    fn phase_10e_checksum_ui_validates_expected_and_avoids_authenticity_claims() {
        let targets = Arc::from([PathBuf::from("/tmp/a")]);
        let request = build_checksum_request(
            Arc::clone(&targets),
            &ChecksumDialogInput {
                algorithm: ChecksumAlgorithm::Sha256,
                expected: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                    .to_owned(),
            },
        )
        .expect("request");
        assert_eq!(request.targets(), targets.as_ref());
        assert_eq!(
            build_checksum_request(
                Arc::from([PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]),
                &ChecksumDialogInput {
                    algorithm: ChecksumAlgorithm::Md5Legacy,
                    expected: "900150983cd24fb0d6963f7d28e17f72".to_owned(),
                }
            ),
            Err(ChecksumDialogError::Request(
                ChecksumRequestError::ExpectedRequiresSingleTarget
            ))
        );
        let presentation = present_checksum(&ChecksumOutcome {
            items: Arc::from([ChecksumItemResult {
                path: PathBuf::from("/tmp/a"),
                algorithm: ChecksumAlgorithm::Md5Legacy,
                digest: "900150983cd24fb0d6963f7d28e17f72".to_owned(),
                bytes: 3,
                verification: ChecksumVerification::Mismatch {
                    expected: "00000000000000000000000000000000".to_owned(),
                },
            }]),
            total_bytes: 3,
        });
        assert!(presentation.algorithm_label.contains("legacy"));
        assert!(presentation.rows[0].verification.contains("Does not match"));
        assert!(presentation.notice.contains("does not prove authenticity"));
        assert_eq!(presentation.copy_text, presentation.rows[0].digest);
    }
}
