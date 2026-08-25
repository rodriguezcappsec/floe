use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

pub const CHECKSUM_TARGET_CAPACITY: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChecksumAlgorithm {
    Sha256,
    Sha512,
    Md5Legacy,
}

impl ChecksumAlgorithm {
    pub const fn digest_bytes(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha512 => 64,
            Self::Md5Legacy => 16,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
            Self::Md5Legacy => "MD5 (legacy compatibility only)",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedDigest(Vec<u8>);

impl ExpectedDigest {
    pub fn parse(algorithm: ChecksumAlgorithm, value: &str) -> Result<Self, ChecksumRequestError> {
        if value.len() != algorithm.digest_bytes() * 2
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ChecksumRequestError::InvalidExpectedDigest(algorithm));
        }
        let mut bytes = Vec::with_capacity(algorithm.digest_bytes());
        for pair in value.as_bytes().chunks_exact(2) {
            let high = decode_hex(pair[0])
                .ok_or(ChecksumRequestError::InvalidExpectedDigest(algorithm))?;
            let low = decode_hex(pair[1])
                .ok_or(ChecksumRequestError::InvalidExpectedDigest(algorithm))?;
            bytes.push((high << 4) | low);
        }
        Ok(Self(bytes))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn canonical_hex(&self) -> String {
        encode_hex(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChecksumRequest {
    targets: Vec<PathBuf>,
    algorithm: ChecksumAlgorithm,
    expected: Option<ExpectedDigest>,
}

impl ChecksumRequest {
    pub fn new(
        targets: Vec<PathBuf>,
        algorithm: ChecksumAlgorithm,
        expected: Option<ExpectedDigest>,
    ) -> Result<Self, ChecksumRequestError> {
        if targets.is_empty() || targets.len() > CHECKSUM_TARGET_CAPACITY {
            return Err(ChecksumRequestError::InvalidTargetCount);
        }
        if expected.is_some() && targets.len() != 1 {
            return Err(ChecksumRequestError::ExpectedRequiresSingleTarget);
        }
        let mut seen = HashSet::with_capacity(targets.len());
        for target in &targets {
            validate_target(target)?;
            if !seen.insert(target.clone()) {
                return Err(ChecksumRequestError::Duplicate(target.clone()));
            }
        }
        Ok(Self {
            targets,
            algorithm,
            expected,
        })
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }

    pub const fn algorithm(&self) -> ChecksumAlgorithm {
        self.algorithm
    }

    pub fn expected(&self) -> Option<&ExpectedDigest> {
        self.expected.as_ref()
    }
}

fn validate_target(target: &Path) -> Result<(), ChecksumRequestError> {
    if !target.is_absolute() {
        return Err(ChecksumRequestError::Relative(target.to_path_buf()));
    }
    if target.file_name().is_none() {
        return Err(ChecksumRequestError::ProtectedRoot(target.to_path_buf()));
    }
    if target.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    }) {
        return Err(ChecksumRequestError::Unnormalized(target.to_path_buf()));
    }
    Ok(())
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ChecksumRequestError {
    #[error("select between one and {CHECKSUM_TARGET_CAPACITY} checksum targets")]
    InvalidTargetCount,
    #[error("checksum target must be absolute: {}", .0.display())]
    Relative(PathBuf),
    #[error("filesystem roots cannot be checksum targets: {}", .0.display())]
    ProtectedRoot(PathBuf),
    #[error("checksum target is not lexically normalized: {}", .0.display())]
    Unnormalized(PathBuf),
    #[error("duplicate checksum target: {}", .0.display())]
    Duplicate(PathBuf),
    #[error("expected checksum verification requires exactly one target")]
    ExpectedRequiresSingleTarget,
    #[error("expected digest is not valid hexadecimal for {}", .0.display_name())]
    InvalidExpectedDigest(ChecksumAlgorithm),
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    #[test]
    fn phase_10e_checksum_request_preserves_exact_paths_and_validates_digests() {
        let raw = OsString::from_vec(b"/tmp/checksum-\xff".to_vec());
        let path = PathBuf::from(raw);
        let expected = ExpectedDigest::parse(
            ChecksumAlgorithm::Sha256,
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
        )
        .expect("uppercase expected digest");
        assert_eq!(
            expected.canonical_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let request = ChecksumRequest::new(
            vec![path.clone()],
            ChecksumAlgorithm::Sha256,
            Some(expected),
        )
        .expect("request");
        assert_eq!(request.targets(), &[path]);
        assert_eq!(request.expected().expect("expected").bytes().len(), 32);
        assert!(matches!(
            ExpectedDigest::parse(ChecksumAlgorithm::Sha512, "abcd"),
            Err(ChecksumRequestError::InvalidExpectedDigest(
                ChecksumAlgorithm::Sha512
            ))
        ));
        assert_eq!(
            ChecksumRequest::new(
                vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
                ChecksumAlgorithm::Md5Legacy,
                Some(
                    ExpectedDigest::parse(
                        ChecksumAlgorithm::Md5Legacy,
                        "900150983cd24fb0d6963f7d28e17f72"
                    )
                    .expect("md5")
                )
            ),
            Err(ChecksumRequestError::ExpectedRequiresSingleTarget)
        );
        assert!(matches!(
            ChecksumRequest::new(
                vec![PathBuf::from("/tmp/../etc/passwd")],
                ChecksumAlgorithm::Sha256,
                None
            ),
            Err(ChecksumRequestError::Unnormalized(_))
        ));
    }
}
