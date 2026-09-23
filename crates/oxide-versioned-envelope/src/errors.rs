//! Error types for this crate.

use crate::wire::{VERSION_FIELD, VERSIONED_DATA_FIELD};
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

    /// The envelope is ill-formed because either the `version` or the
    /// `versioned_data` field appears more than once.
    DuplicateFields {
        /// The field or fields that appear more than once.
        fields: EnvelopeFieldSet,
    },

    /// Field names other than `version` and `versioned_data` were found.
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
            Self::Json { .. } => {
                f.write_str("the input could not be parsed as JSON")
            }
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
                    "the envelope has more than one `{VERSIONED_DATA_FIELD}` \
                     field"
                ),
                EnvelopeFieldSet::VersionAndData => write!(
                    f,
                    "the envelope has more than one `{VERSION_FIELD}` field \
                     and more than one `{VERSIONED_DATA_FIELD}` field"
                ),
            },
            Self::UnknownFields { names } => {
                write!(
                    f,
                    "the envelope has fields other than `{VERSION_FIELD}` \
                     and `{VERSIONED_DATA_FIELD}`: "
                )?;
                for (index, name) in
                    names.iter().take(MAX_UNKNOWN_FIELDS_SHOWN).enumerate()
                {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name:?}")?;
                }
                let hidden =
                    names.len().saturating_sub(MAX_UNKNOWN_FIELDS_SHOWN);
                if hidden > 0 {
                    write!(f, ", and {hidden} more")?;
                }
                Ok(())
            }
            Self::Version { .. } => write!(
                f,
                "the envelope's `{VERSION_FIELD}` is not a valid version \
                 number (an integer from 0 to {})",
                u32::MAX
            ),
        }
    }
}

/// This keeps the unknown fields message readable when a JSON payload has many
/// unknown fields.
const MAX_UNKNOWN_FIELDS_SHOWN: usize = 8;

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

    /// The `versioned_data` field.
    Data,

    /// Both the `version` and the `versioned_data` fields.
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
                    write!(
                        f,
                        "it is an object with no `{VERSIONED_DATA_FIELD}` field"
                    )
                }
                EnvelopeFieldSet::VersionAndData => write!(
                    f,
                    "it is an object with neither a `{VERSION_FIELD}` nor a \
                     `{VERSIONED_DATA_FIELD}` field"
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
                "the envelope's `{VERSIONED_DATA_FIELD}` is not a version \
                 {version} payload"
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
            Self::Json { .. } => {
                f.write_str("the input could not be parsed as JSON")
            }
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

/// An error returned by [`read_json_or_else`](crate::read_json_or_else).
///
/// This error indicates that reading an envelope failed, or that a fallback
/// path was encountered and that failed.
#[derive(Debug)]
pub enum ReadOrFallbackError {
    /// The read failed.
    Read(ReadError),

    /// The data was not an envelope, and the fallback failed.
    ///
    /// `source` is the error returned by the fallback.
    Fallback {
        /// The reason the data was not an envelope.
        not_an_envelope: NotAnEnvelopeReason,

        /// The error returned by the fallback.
        source: Box<dyn Error + Send + Sync>,
    },
}

impl fmt::Display for ReadOrFallbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => fmt::Display::fmt(error, f),
            Self::Fallback { not_an_envelope, .. } => write!(
                f,
                "the input is not a versioned envelope ({not_an_envelope}), \
                 and reading it as a pre-envelope document failed"
            ),
        }
    }
}

impl Error for ReadOrFallbackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => error.source(),
            Self::Fallback { source, .. } => Some(&**source),
        }
    }
}

/// An error returned by [`read_json_or_untagged`](crate::read_json_or_untagged).
///
/// This error indicates that reading a JSON envelope failed, or that the data
/// is not an envelope and untagged deserialization failed.
#[derive(Debug)]
pub enum ReadOrUntaggedError {
    /// The read failed.
    Read(ReadError),

    /// The data was not an envelope, and untagged deserialization failed.
    ///
    /// `source` is the error returned by the fallback.
    Untagged {
        /// The reason the data was not an envelope.
        not_an_envelope: NotAnEnvelopeReason,

        /// The error returned by untagged deserialization.
        source: UntaggedError,
    },
}

impl fmt::Display for ReadOrUntaggedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => fmt::Display::fmt(error, f),
            Self::Untagged { not_an_envelope, .. } => write!(
                f,
                "the input is not a versioned envelope ({not_an_envelope}), \
                 and reading it as a pre-envelope document failed"
            ),
        }
    }
}

impl Error for ReadOrUntaggedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => error.source(),
            Self::Untagged { source, .. } => Some(source),
        }
    }
}

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
                if index + 1 != self.0.len() {
                    f.write_str(", ")?;
                } else if self.0.len() > 2 {
                    f.write_str(", and ")?;
                } else {
                    f.write_str(" and ")?;
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
    original: u32,
    from: u32,
    to: u32,
    source: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for ConversionFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { original, from, to, source: _ } = self;
        if original == from {
            write!(
                f,
                "converting a version {from} payload to version {to} failed"
            )
        } else {
            write!(
                f,
                "converting a version {original} payload failed at the step \
                 from version {from} to version {to}"
            )
        }
    }
}

impl Error for ConversionFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

impl ConversionFailed {
    pub(crate) fn new(
        original: u32,
        from: u32,
        to: u32,
        source: Box<dyn Error + Send + Sync>,
    ) -> Self {
        Self { original, from, to, source }
    }

    /// The original version of the payload, before any upgrade steps.
    pub fn original_version(&self) -> u32 {
        self.original
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
            "the input is not a payload of any version this reader \
             understands:",
        )?;

        // Versions usually share fields, so collapse identical errors. The
        // first-seen order here ensures the newest version is shown first.
        let mut groups: Vec<(String, Vec<u32>)> = Vec::new();
        for attempt in &self.attempts {
            let message = attempt.error.to_string();
            match groups.iter_mut().find(|(seen, _)| *seen == message) {
                Some((_, versions)) => versions.push(attempt.version),
                None => groups.push((message, vec![attempt.version])),
            }
        }

        for (message, versions) in &groups {
            let noun = if versions.len() == 1 { "version" } else { "versions" };
            write!(f, "\n  - {noun} {}: {message}", VersionList(versions))?;
        }
        Ok(())
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
