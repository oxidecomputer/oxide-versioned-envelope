//! Error types for this crate.

use crate::wire::{DATA_FIELD, VERSION_FIELD};
use std::{collections::BTreeSet, error::Error, fmt};

/// An error that occurs while deserializing data with a versioned envelope.
///
/// Part of [`ReadError`].
#[derive(Debug)]
pub enum EnvelopeError {
    /// An error occurred while deserializing the JSON document.
    Json {
        /// The underlying JSON error.
        source: serde_json::Error,
    },

    /// The input is not a versioned envelope.
    NotAnEnvelope {
        /// The reason the input is not a versioned envelope.
        reason: NotAnEnvelopeReason,
    },

    /// The envelope is ill-formed because either the `version` or the `data`
    /// field appears more than once.
    DuplicateFields {
        /// The field or fields that appear more than once.
        fields: EnvelopeFieldSet,
    },

    /// Field names other than `version` and `data` were found.
    UnknownFields {
        /// The unknown field names.
        names: BTreeSet<String>,
    },

    /// Deserializing the version field failed.
    Version {
        /// The error that occurred.
        source: serde_json::Error,
    },
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json { .. } => f.write_str("the input is not valid JSON"),
            Self::NotAnEnvelope { reason } => write!(
                f,
                "the input is valid JSON, but not a versioned envelope: \
                 {reason}"
            ),
            Self::DuplicateFields { fields } => match fields {
                EnvelopeFieldSet::Version => write!(
                    f,
                    "the envelope has more than one `{VERSION_FIELD}` field"
                ),
                EnvelopeFieldSet::Data => write!(
                    f,
                    "the envelope has more than one `{DATA_FIELD}` field"
                ),
                EnvelopeFieldSet::VersionAndData => write!(
                    f,
                    "the envelope has more than one `{VERSION_FIELD}` field \
                     and more than one `{DATA_FIELD}` field"
                ),
            },
            Self::UnknownFields { names } => {
                write!(
                    f,
                    "the envelope has fields other than `{VERSION_FIELD}` \
                     and `{DATA_FIELD}`: "
                )?;
                for (index, name) in names.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "`{name}`")?;
                }
                Ok(())
            }
            Self::Version { .. } => {
                write!(f, "the envelope's {VERSION_FIELD} is not a u32")
            }
        }
    }
}

impl Error for EnvelopeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json { source } | Self::Version { source } => Some(source),
            Self::NotAnEnvelope { .. }
            | Self::DuplicateFields { .. }
            | Self::UnknownFields { .. } => None,
        }
    }
}

/// Which envelope field or fields an error is about.
///
/// This is part of [`NotAnEnvelopeReason::Missing`] and
/// [`EnvelopeError::DuplicateFields`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnvelopeFieldSet {
    /// The `version` field.
    Version,

    /// The `data` field.
    Data,

    /// Both the `version` and the `data` fields.
    VersionAndData,
}

/// The reason a JSON blob does not appear to be an envelope.
///
/// Part of [`EnvelopeError::NotAnEnvelope`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NotAnEnvelopeReason {
    /// The root is not a JSON object.
    NonObjectRoot {
        /// The non-object kind the root is.
        kind: NonObjectKind,
    },

    /// One or more required fields are missing.
    Missing {
        /// The fields that are missing.
        fields: EnvelopeFieldSet,
    },
}

impl fmt::Display for NotAnEnvelopeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonObjectRoot { kind } => {
                let kind = match kind {
                    NonObjectKind::Null => "null",
                    NonObjectKind::Bool => "a boolean",
                    NonObjectKind::Number => "a number",
                    NonObjectKind::String => "a string",
                    NonObjectKind::Array => "an array",
                };
                write!(f, "its root is {kind}, not an object")
            }
            Self::Missing { fields } => match fields {
                EnvelopeFieldSet::Version => {
                    write!(f, "it is an object with no `{VERSION_FIELD}` field")
                }
                EnvelopeFieldSet::Data => {
                    write!(f, "it is an object with no `{DATA_FIELD}` field")
                }
                EnvelopeFieldSet::VersionAndData => write!(
                    f,
                    "it is an object with neither a `{VERSION_FIELD}` nor a \
                     `{DATA_FIELD}` field"
                ),
            },
        }
    }
}

/// The non-object JSON kind a particular blob is.
///
/// Part of [`NotAnEnvelopeReason::NonObjectRoot`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NonObjectKind {
    /// The root is null.
    Null,

    /// The root is a boolean.
    Bool,

    /// The root is a number.
    Number,

    /// The root is a string.
    String,

    /// The root is an array.
    Array,
}

/// An error produced while reading a JSON blob.
///
/// Produced by the `read_*` functions in this crate.
#[derive(Debug)]
pub enum ReadError {
    /// There was an error reading the envelope.
    Envelope(EnvelopeError),

    /// Deserializing the version identified in the envelope is not supported.
    UnsupportedVersion(UnsupportedVersion),

    /// There was an error deserializing the data at the version specified in
    /// the envelope.
    Data {
        /// The version of the data that could not be deserialized.
        version: u32,

        /// The underlying error.
        source: serde_json::Error,
    },

    /// Conversion to a newer version failed.
    Conversion(ConversionFailed),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Treat wrapped errors as transparent.
            Self::Envelope(error) => fmt::Display::fmt(error, f),
            Self::UnsupportedVersion(error) => fmt::Display::fmt(error, f),
            Self::Data { version, .. } => write!(
                f,
                "the envelope's `{DATA_FIELD}` is not a version {version} \
                 payload"
            ),
            Self::Conversion(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Envelope(error) => error.source(),
            Self::UnsupportedVersion(error) => error.source(),
            Self::Data { source, .. } => Some(source),
            Self::Conversion(error) => error.source(),
        }
    }
}

/// The error produced while deserializing a payload that isn't inside an
/// envelope.
///
/// Returned by [`read_untagged_json`](crate::read_untagged_json).
#[derive(Debug)]
pub enum UntaggedError {
    /// An error occurred deserializing the JSON blob.
    Json {
        /// The underlying error.
        source: serde_json::Error,
    },

    /// No version's deserializer accepted this JSON blob.
    NoMatchingVersion(NoMatchingVersion),

    /// The data could not be converted to the latest version.
    Conversion(ConversionFailed),
}

impl fmt::Display for UntaggedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json { .. } => f.write_str("the input is not valid JSON"),
            Self::NoMatchingVersion(error) => fmt::Display::fmt(error, f),
            Self::Conversion(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl Error for UntaggedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json { source } => Some(source),
            Self::NoMatchingVersion(error) => error.source(),
            Self::Conversion(error) => error.source(),
        }
    }
}

/// An error indicating that reading a JSON envelope failed, or that a fallback
/// path was encountered and that failed.
///
/// Returned by:
///
/// * [`read_json_or_else`](crate::read_json_or_else) and
///   [`read_json_or_else_with`](crate::read_json_or_else_with), whose callback
///   returns the type parameter `E` as its error type.
/// * [`read_json_or_untagged`](crate::read_json_or_untagged), which returns the
///   [`ReadOrUntaggedError`] type alias.
#[derive(Debug)]
pub enum ReadOrFallbackError<E> {
    /// The read failed.
    Read(ReadError),

    /// The data was not an envelope, and the fallback failed.
    ///
    /// `source` is the error returned by the fallback.
    Fallback {
        /// The reason the data was not an envelope.
        not_an_envelope: NotAnEnvelopeReason,

        /// The error returned by the fallback.
        source: E,
    },
}

impl<E> fmt::Display for ReadOrFallbackError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => fmt::Display::fmt(error, f),
            Self::Fallback { not_an_envelope, .. } => write!(
                f,
                "the input is valid JSON, but not a versioned envelope \
                 ({not_an_envelope}), so it was read as a document from \
                 before the envelope, and that failed"
            ),
        }
    }
}

impl<E: Error + 'static> Error for ReadOrFallbackError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => error.source(),
            Self::Fallback { source, .. } => Some(source),
        }
    }
}

/// A type alias for the error type returned by
/// [`read_json_or_untagged`](crate::read_json_or_untagged).
pub type ReadOrUntaggedError = ReadOrFallbackError<UntaggedError>;

/// A version is unsupported on the deserialize side.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnsupportedVersion {
    found: u32,
    supported: &'static [u32],
}

impl UnsupportedVersion {
    pub(crate) fn new(found: u32, supported: &'static [u32]) -> Self {
        Self { found, supported }
    }

    /// Returns the version identified by the envelope.
    pub fn found(&self) -> u32 {
        self.found
    }

    /// Returns the list of supported versions.
    pub fn supported(&self) -> &'static [u32] {
        self.supported
    }

    /// Returns information about the relative position of this version.
    pub fn kind(&self) -> UnsupportedVersionKind {
        let expected = "the supported versions are non-empty, which \
                        assert_version_set_well_formed established";
        let oldest = *self.supported.first().expect(expected);
        let latest = *self.supported.last().expect(expected);
        if self.found > latest {
            UnsupportedVersionKind::Newer { latest }
        } else if self.found < oldest {
            UnsupportedVersionKind::Older { oldest }
        } else {
            UnsupportedVersionKind::Gap
        }
    }
}

impl fmt::Display for UnsupportedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let found = self.found;
        match self.kind() {
            UnsupportedVersionKind::Newer { latest } => write!(
                f,
                "the envelope's version is {found}, but the newest \
                 version this reader understands is {latest}"
            ),
            UnsupportedVersionKind::Older { oldest } => write!(
                f,
                "the envelope's version is {found}, but the oldest \
                 version this reader supports is {oldest}"
            ),
            UnsupportedVersionKind::Gap => write!(
                f,
                "the envelope's version is {found}, but this reader \
                 understands only versions {}",
                VersionList(self.supported)
            ),
        }
    }
}

impl Error for UnsupportedVersion {}

/// The relative position of an unsupported version.
///
/// Returned by [`UnsupportedVersion::kind`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnsupportedVersionKind {
    /// The unsupported version is newer than the latest version supported by
    /// the deserializer.
    Newer {
        /// The latest supported version.
        latest: u32,
    },
    /// The unsupported version is older than the oldest version supported by
    /// the deserializer.
    Older {
        /// The oldest supported version.
        oldest: u32,
    },
    /// The unsupported version is between two supported versions.
    Gap,
}

struct VersionList<'a>(&'a [u32]);

impl fmt::Display for VersionList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, version) in self.0.iter().enumerate() {
            if index > 0 {
                if index + 1 == self.0.len() {
                    f.write_str(" and ")?;
                } else {
                    f.write_str(", ")?;
                }
            }
            write!(f, "{version}")?;
        }
        Ok(())
    }
}

/// Conversion to a newer version failed.
///
/// Part of [`ReadError::Conversion`] and [`UntaggedError::Conversion`].
#[derive(Debug)]
pub struct ConversionFailed {
    from: u32,
    to: u32,
    source: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for ConversionFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "converting a version {} payload to version {} failed",
            self.from, self.to
        )
    }
}

impl Error for ConversionFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

impl ConversionFailed {
    pub(crate) fn new(
        from: u32,
        to: u32,
        source: Box<dyn Error + Send + Sync>,
    ) -> Self {
        Self { from, to, source }
    }

    /// Returns the version from which conversion failed.
    ///
    /// In case of multi-step upgrades, this can be any of the versions along
    /// the way.
    pub fn from_version(&self) -> u32 {
        self.from
    }

    /// Returns the version that conversion failed to.
    pub fn to_version(&self) -> u32 {
        self.to
    }
}

/// For an untagged deserialize, a JSON blob was rejected by every version's
/// deserializer.
///
/// Part of [`UntaggedError::NoMatchingVersion`].
#[derive(Debug)]
pub struct NoMatchingVersion {
    attempts: Vec<VersionAttempt>,
}

impl NoMatchingVersion {
    pub(crate) fn new(attempts: Vec<VersionAttempt>) -> Self {
        Self { attempts }
    }

    /// Returns information about every deserialize attempt that was tried and
    /// why that failed.
    pub fn attempts(&self) -> &[VersionAttempt] {
        &self.attempts
    }
}

impl fmt::Display for NoMatchingVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the document carries no version, and is not a payload of any \
             version this reader understands (",
        )?;
        for (index, attempt) in self.attempts.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "version {}: {}", attempt.version, attempt.error)?;
        }
        f.write_str(")")
    }
}

impl Error for NoMatchingVersion {
    // No source here, since attempts can list out many attempts.
}

/// An error during an attempt trying to deserialize untagged data.
///
/// Returned by [`NoMatchingVersion::attempts`].
#[derive(Debug)]
pub struct VersionAttempt {
    version: u32,
    error: serde_json::Error,
}

impl VersionAttempt {
    pub(crate) fn new(version: u32, error: serde_json::Error) -> Self {
        Self { version, error }
    }

    /// Returns the version deserialization was tried with.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the error that occurred.
    pub fn error(&self) -> &serde_json::Error {
        &self.error
    }
}
