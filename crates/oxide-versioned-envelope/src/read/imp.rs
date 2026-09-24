//! Support for reading versioned envelopes.
//!
//! A naive implementation would do something like:
//!
//! ```rust
//! struct ReadEnvelope {
//!     version: u32,
//!     data: serde_json::Value,
//! }
//!
//! impl ReadEnvelope {
//!     fn deserialize_data<T: serde::de::DeserializeOwned>(
//!         &self,
//!     ) -> Result<T, serde_json::Error> {
//!         serde_json::from_value(self.data.clone())
//!     }
//! }
//! ```
//!
//! But this has a few pitfalls:
//!
//! 1. `serde_json::Value` only preserves object key order if the
//!    `preserve_order` feature is enabled. There are a few
//!    different options, all of which are suboptimal:
//!
//!    * Enable the `preserve_order` feature, which affects
//!      all other crates in the graph.
//!    * Accept that key order might not roundtrip.
//!
//!    This dependency is hard to explain to users; we can do much
//!    better than this.
//!
//! 2. `serde_json::Value` cannot represent certain types that
//!    serde itself can deserialize fine, such as `u128`s.
//!
//! 3. If deserialization fails, the error message can't carry
//!    a line/column offset.
//!
//! Instead, we perform passes over the underlying JSON:
//!
//! 1. The first pass validates that the JSON is well-formed.
//! 2. Then, depending on the read method called, we perform one or
//!    more deserialization passes through the [`VersionSet::parse`]
//!    callback.

use super::{
    document::{
        Document, EnvelopeField, Field, ObjectFields, deserialize_field,
    },
    output::{Origin, ReadOutput},
    validate::validate_json,
};
use crate::{
    errors::{
        ConversionFailed, EnvelopeError, EnvelopeFieldSet, NoMatchingVersion,
        NotAnEnvelopeReason, ReadError, ReadOrFallbackError,
        ReadOrUntaggedError, UnsupportedVersion, UntaggedError, VersionAttempt,
    },
    versioned::{Step, VersionSet, assert_version_set_well_formed},
};
use serde_core::de::{DeserializeSeed, Deserializer};
use std::{any::type_name, marker::PhantomData};

fn parse_envelope(bytes: &[u8]) -> Result<u32, EnvelopeError> {
    // Validate the JSON and ensure it is a valid top-level document.
    let document = Document::from_json(bytes)
        .map_err(|source| EnvelopeError::Json { source })?;

    let fields = match document {
        Document::Object(fields) => fields,
        Document::NonObject(kind) => {
            return Err(EnvelopeError::NotAnEnvelope {
                reason: NotAnEnvelopeReason::NonObjectRoot { kind },
            });
        }
    };

    check_fields(fields)?;

    deserialize_field(bytes, EnvelopeField::Version, PhantomData::<u32>)
        .map_err(|source| EnvelopeError::Version { source })
}

fn check_fields(fields: ObjectFields) -> Result<(), EnvelopeError> {
    let missing = |fields| {
        Err(EnvelopeError::NotAnEnvelope {
            reason: NotAnEnvelopeReason::Missing { fields },
        })
    };
    let duplicate = |fields| Err(EnvelopeError::DuplicateFields { fields });

    match (fields.version, fields.data) {
        (Field::Absent, Field::Absent) => {
            return missing(EnvelopeFieldSet::VersionAndData);
        }
        (Field::Absent, Field::Present | Field::Duplicated) => {
            return missing(EnvelopeFieldSet::Version);
        }
        (Field::Present | Field::Duplicated, Field::Absent) => {
            return missing(EnvelopeFieldSet::Data);
        }
        (Field::Duplicated, Field::Present) => {
            return duplicate(EnvelopeFieldSet::Version);
        }
        (Field::Present, Field::Duplicated) => {
            return duplicate(EnvelopeFieldSet::Data);
        }
        (Field::Duplicated, Field::Duplicated) => {
            return duplicate(EnvelopeFieldSet::VersionAndData);
        }
        (Field::Present, Field::Present) => {}
    }

    if !fields.unknown.is_empty() {
        return Err(EnvelopeError::UnknownFields { names: fields.unknown });
    }

    Ok(())
}

struct ParseSeed<S> {
    version: u32,
    marker: PhantomData<fn() -> S>,
}

impl<S> ParseSeed<S> {
    fn new(version: u32) -> Self {
        Self { version, marker: PhantomData }
    }
}

impl<'de, S: VersionSet> DeserializeSeed<'de> for ParseSeed<S> {
    type Value = S;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<S, D::Error> {
        S::parse(self.version, deserializer)
    }
}

/// Deserializes a JSON blob as a versioned envelope of this [`VersionSet`],
/// automatically upgrading the blob to the latest version.
///
/// This can be used in two ways:
///
/// * As a blob at a fixed version, where `S` is a [`Versioned`](crate::Versioned).
///   (There is a blanket implementation to enable this.)
/// * As a blob with an upgradable version, where `S` is a [`VersionSet`].
///
/// # Read functions
///
/// This crate comes with several read functions. Here's an index to determine
/// when to use each:
///
/// | Function | If the input is an envelope | If the input is not an envelope | Error type |
/// |---|---|---|---|
/// | [`read_json`] | Reads and upgrades it | Fails | [`ReadError`] |
/// | [`read_untagged_json`] | Does not look for one | Tries each version's payload, newest first | [`UntaggedError`] |
/// | [`read_json_or_else`] | Reads and upgrades it | Calls your fallback with the [`NotAnEnvelopeReason`] | [`ReadOrFallbackError<E>`](ReadOrFallbackError) |
/// | [`read_json_or_untagged`] | Reads and upgrades it | Tries each version's payload, newest first | [`ReadOrUntaggedError`] |
///
/// Each function has a `_with` variant, such as [`read_json_with`], that passes
/// a [`VersionSet::Context`] to the upgrade steps. (The non-`_with` functions
/// above require `Context = ()`.)
///
/// An envelope that appears to be damaged, such as one with a repeated or
/// unknown field or a bad `version`, is always an error, and fallback paths are
/// not invoked for it.
///
/// # Examples
///
/// With a fixed version:
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Versioned,
///     errors::{ReadError, UnsupportedVersionKind},
///     read_json,
/// };
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize, Eq, PartialEq)]
/// struct Report {
///     passed: u32,
/// }
///
/// impl Versioned for Report {
///     const VERSION: u32 = 2;
/// }
///
/// // Deserialize a versioned envelope at version 2.
/// let current = br#"{"version":2,"data":{"passed":5}}"#;
/// assert_eq!(
///     read_json::<Report>(current)?.into_value(),
///     Report { passed: 5 }
/// );
///
/// // Deserializing with a newer version fails.
/// let from_a_newer_writer =
///     br#"{"version":3,"data":{"passed":5,"skipped":1}}"#;
/// let error = read_json::<Report>(from_a_newer_writer)
///     .expect_err("version 3 is newer than this reader understands");
/// let ReadError::UnsupportedVersion(unsupported) = &error else {
///     panic!("expected an unsupported version, got: {error}");
/// };
/// assert_eq!(unsupported.found(), 3);
/// assert_eq!(unsupported.kind(), UnsupportedVersionKind::Newer { latest: 2 });
/// # Ok(()) }
/// ```
///
/// For an example with upgradable versions, see the [crate-level
/// documentation](crate#as-upgradable-versions).
pub fn read_json<S: VersionSet<Context = ()>>(
    bytes: &[u8],
) -> Result<ReadOutput<S::Latest>, ReadError> {
    read_json_with::<S>(bytes, &())
}

/// A variant of [`read_json`] that can have extra context provided to it.
pub fn read_json_with<S: VersionSet>(
    bytes: &[u8],
    cx: &S::Context,
) -> Result<ReadOutput<S::Latest>, ReadError> {
    let () = const { assert_version_set_well_formed::<S>() };

    let found = parse_envelope(bytes).map_err(ReadError::Envelope)?;

    if !is_member::<S>(found) {
        return Err(ReadError::UnsupportedVersion(UnsupportedVersion::new(
            found,
            S::VERSIONS,
        )));
    }

    let value = match deserialize_field(
        bytes,
        EnvelopeField::Data,
        ParseSeed::<S>::new(found),
    ) {
        Ok(value) => value,
        Err(source) => {
            return Err(ReadError::Data { version: found, source });
        }
    };

    let latest = upgrade(value, found, cx).map_err(ReadError::Conversion)?;
    Ok(ReadOutput::new(
        latest,
        Origin::Envelope { version: found },
        latest_version::<S>(),
    ))
}

/// Attempts to deserialize the payload without a versioned envelope being
/// present.
///
/// This function attempts to deserialize a payload, starting from the latest
/// version and working backwards through the set. This is a kind of duck-typed
/// versioning, which is inherently fragile and is only recommended for legacy
/// blobs.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Versioned, errors::UntaggedError, read_untagged_json, version_set,
/// };
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct ProfileV1 {
///     name: String,
/// }
///
/// #[derive(Debug, Deserialize, Eq, PartialEq)]
/// struct ProfileV2 {
///     name: String,
///     display_name: Option<String>,
/// }
///
/// impl Versioned for ProfileV1 {
///     const VERSION: u32 = 1;
/// }
///
/// impl Versioned for ProfileV2 {
///     const VERSION: u32 = 2;
/// }
///
/// impl From<ProfileV1> for ProfileV2 {
///     fn from(v1: ProfileV1) -> Self {
///         Self { display_name: Some(v1.name.clone()), name: v1.name }
///     }
/// }
///
/// version_set! {
///     enum Profile -> ProfileV2 {
///         V1(ProfileV1) => V2,
///         V2(ProfileV2),
///     }
/// }
///
/// // This read succeeds.
/// let written_as_v1 = br#"{"name":"foo"}"#;
/// assert_eq!(
///     read_untagged_json::<Profile>(written_as_v1)?.into_value(),
///     ProfileV2 { name: "foo".to_owned(), display_name: None },
///     "version 2 is tried first and parses the document, so the conversion \
///      from version 1 never runs",
/// );
///
/// // This read fails.
/// let error = read_untagged_json::<Profile>(br#"{"nickname":"foo"}"#)
///     .expect_err("neither version has a `nickname` field");
/// let UntaggedError::NoMatchingVersion(none) = &error else {
///     panic!("expected no matching version, got: {error}");
/// };
/// assert_eq!(
///     none.attempts()
///         .iter()
///         .map(|attempt| attempt.version())
///         .collect::<Vec<_>>(),
///     [2, 1],
/// );
/// # Ok(()) }
/// ```
pub fn read_untagged_json<S: VersionSet<Context = ()>>(
    bytes: &[u8],
) -> Result<ReadOutput<S::Latest>, UntaggedError> {
    read_untagged_json_with::<S>(bytes, &())
}

/// A variant of [`read_untagged_json`] that can have extra context provided to
/// it.
pub fn read_untagged_json_with<S: VersionSet>(
    bytes: &[u8],
    cx: &S::Context,
) -> Result<ReadOutput<S::Latest>, UntaggedError> {
    let () = const { assert_version_set_well_formed::<S>() };

    validate_json(bytes).map_err(|source| UntaggedError::Json { source })?;

    let versions = S::VERSIONS;
    let mut attempts = Vec::new();

    for &version in versions.iter().rev() {
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let parsed = S::parse(version, &mut deserializer)
            .and_then(|value| deserializer.end().map(|()| value));
        match parsed {
            Ok(value) => {
                let latest = upgrade(value, version, cx)
                    .map_err(UntaggedError::Conversion)?;
                return Ok(ReadOutput::new(
                    latest,
                    Origin::Untagged { version },
                    latest_version::<S>(),
                ));
            }
            Err(source) => {
                attempts.push(VersionAttempt::new(version, source));
            }
        }
    }

    Err(UntaggedError::NoMatchingVersion(NoMatchingVersion::new(attempts)))
}

/// Attempts to deserialize a JSON blob, first attempting to interpret it as a
/// versioned envelope, and calling the callback if it does not appear to be an
/// envelope.
///
/// Any JSON object with `version` and `data` fields is treated as an envelope.
/// If a JSON object appears to be an envelope but it fails to deserialize, the
/// fallback path is not invoked.
///
/// # Examples
///
/// If it's not a versioned envelope, fall back to a custom parser:
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Origin, ReadOutput, Versioned, WriteEnvelope,
///     errors::{
///         NonObjectKind, NotAnEnvelopeReason, ReadError, ReadOrFallbackError,
///     },
///     read_json_or_else,
/// };
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
/// struct Settings {
///     verbose: bool,
/// }
///
/// impl Versioned for Settings {
///     const VERSION: u32 = 1;
/// }
///
/// fn read_settings(
///     bytes: &[u8],
/// ) -> Result<ReadOutput<Settings>, ReadOrFallbackError<serde_json::Error>>
/// {
///     read_json_or_else::<Settings, _, _>(bytes, |_reason| {
///         // As the fallback, attempt to read the bytes as a plain bool (true
///         // or false).
///         let verbose = serde_json::from_slice::<bool>(bytes)?;
///         Ok(Settings { verbose })
///     })
/// }
///
/// let read = read_settings(br#"{"version":1,"data":{"verbose":true}}"#)?;
/// assert_eq!(read.origin(), Origin::Envelope { version: 1 });
/// // Data inside an envelope does not need a rewrite.
/// assert!(!read.needs_rewrite());
/// assert_eq!(read.into_value(), Settings { verbose: true });
///
/// // A plain bool does need a rewrite.
/// let read = read_settings(b"true")?;
/// assert_eq!(
///     read.origin(),
///     Origin::Fallback {
///         not_an_envelope: NotAnEnvelopeReason::NonObjectRoot {
///             kind: NonObjectKind::Bool,
///         },
///     },
/// );
/// assert!(read.needs_rewrite());
///
/// let rewritten = serde_json::to_string(&WriteEnvelope::new(read.value()))?;
/// assert_eq!(rewritten, r#"{"version":1,"data":{"verbose":true}}"#);
///
/// // If the data is a versioned envelope, but it cannot be parsed, the
/// // fallback is not invoked.
/// let error = read_settings(br#"{"version":1,"data":{"verbose":"yes"}}"#)
///     .expect_err("`verbose` is not a bool");
/// let ReadOrFallbackError::Read(ReadError::Data { version, .. }) = &error
/// else {
///     panic!("expected the envelope's data to be refused, got: {error}");
/// };
/// assert_eq!(*version, 1);
/// # Ok(()) }
/// ```
pub fn read_json_or_else<S, F, E>(
    bytes: &[u8],
    fallback: F,
) -> Result<ReadOutput<S::Latest>, ReadOrFallbackError<E>>
where
    S: VersionSet<Context = ()>,
    F: FnOnce(NotAnEnvelopeReason) -> Result<S::Latest, E>,
{
    read_json_or_else_with::<S, F, E>(bytes, &(), fallback)
}

/// A variant of [`read_json_or_else`] that can have extra context provided to
/// it.
pub fn read_json_or_else_with<S, F, E>(
    bytes: &[u8],
    cx: &S::Context,
    fallback: F,
) -> Result<ReadOutput<S::Latest>, ReadOrFallbackError<E>>
where
    S: VersionSet,
    F: FnOnce(NotAnEnvelopeReason) -> Result<S::Latest, E>,
{
    read_json_or_else_output_with::<S, _, E>(bytes, cx, |not_an_envelope| {
        let latest = fallback(not_an_envelope)?;
        Ok(ReadOutput::new(
            latest,
            Origin::Fallback { not_an_envelope },
            latest_version::<S>(),
        ))
    })
}

/// Attempts to deserialize a JSON blob, first attempting to interpret it as a
/// versioned envelope, and falling back to [`read_untagged_json`] if it does
/// not appear to be an envelope.
///
/// Any JSON object with `version` and `data` fields is treated as an envelope.
/// If a JSON object appears to be an envelope but it fails to deserialize, the
/// untagged path is not invoked.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Origin, Versioned,
///     errors::{ReadError, ReadOrFallbackError},
///     read_json_or_untagged, version_set,
/// };
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct ThresholdsV1 {
///     warn: u32,
/// }
///
/// #[derive(Debug, Deserialize, Eq, PartialEq)]
/// struct ThresholdsV2 {
///     warn: u32,
///     critical: u32,
/// }
///
/// impl Versioned for ThresholdsV1 {
///     const VERSION: u32 = 1;
/// }
///
/// impl Versioned for ThresholdsV2 {
///     const VERSION: u32 = 2;
/// }
///
/// impl From<ThresholdsV1> for ThresholdsV2 {
///     fn from(v1: ThresholdsV1) -> Self {
///         Self { warn: v1.warn, critical: v1.warn * 2 }
///     }
/// }
///
/// // Define a version set.
/// version_set! {
///     enum Thresholds -> ThresholdsV2 {
///         V1(ThresholdsV1) => V2,
///         V2(ThresholdsV2),
///     }
/// }
///
/// // Read a versioned envelope at the latest version.
/// let read = read_json_or_untagged::<Thresholds>(
///     br#"{"version":2,"data":{"warn":10,"critical":50}}"#,
/// )?;
/// assert_eq!(read.origin(), Origin::Envelope { version: 2 });
/// assert!(!read.needs_rewrite());
/// assert_eq!(read.into_value(), ThresholdsV2 { warn: 10, critical: 50 });
///
/// // Read a versioned envelope, upgrading it automatically.
/// let read = read_json_or_untagged::<Thresholds>(
///     br#"{"version":1,"data":{"warn":10}}"#,
/// )?;
/// assert_eq!(read.origin(), Origin::Envelope { version: 1 });
/// assert!(read.needs_rewrite());
/// assert_eq!(read.into_value(), ThresholdsV2 { warn: 10, critical: 20 });
///
/// // Read an untagged JSON blob.
/// let read = read_json_or_untagged::<Thresholds>(br#"{"warn":10}"#)?;
/// assert_eq!(read.origin(), Origin::Untagged { version: 1 });
/// assert!(read.needs_rewrite());
/// assert_eq!(read.into_value(), ThresholdsV2 { warn: 10, critical: 20 });
///
/// // Reading an envelope that's too new fails.
/// let error = read_json_or_untagged::<Thresholds>(
///     br#"{"version":3,"data":{"warn":10}}"#,
/// )
/// .expect_err("version 3 is newer than this reader understands");
/// let ReadOrFallbackError::Read(ReadError::UnsupportedVersion(unsupported)) =
///     &error
/// else {
///     panic!("expected an unsupported version, got: {error}");
/// };
/// assert_eq!(unsupported.found(), 3);
/// # Ok(()) }
/// ```
pub fn read_json_or_untagged<S: VersionSet<Context = ()>>(
    bytes: &[u8],
) -> Result<ReadOutput<S::Latest>, ReadOrUntaggedError> {
    read_json_or_untagged_with::<S>(bytes, &())
}

/// A variant of [`read_json_or_untagged`] that can have extra context provided
/// to it.
pub fn read_json_or_untagged_with<S: VersionSet>(
    bytes: &[u8],
    cx: &S::Context,
) -> Result<ReadOutput<S::Latest>, ReadOrUntaggedError> {
    read_json_or_else_output_with::<S, _, _>(bytes, cx, |_not_an_envelope| {
        read_untagged_json_with::<S>(bytes, cx)
    })
}

fn read_json_or_else_output_with<S, F, E>(
    bytes: &[u8],
    cx: &S::Context,
    fallback: F,
) -> Result<ReadOutput<S::Latest>, ReadOrFallbackError<E>>
where
    S: VersionSet,
    F: FnOnce(NotAnEnvelopeReason) -> Result<ReadOutput<S::Latest>, E>,
{
    let not_an_envelope = match read_json_with::<S>(bytes, cx) {
        Ok(output) => return Ok(output),
        Err(error) => {
            not_an_envelope(error).map_err(ReadOrFallbackError::Read)?
        }
    };

    fallback(not_an_envelope).map_err(|source| ReadOrFallbackError::Fallback {
        not_an_envelope,
        source,
    })
}

fn not_an_envelope(error: ReadError) -> Result<NotAnEnvelopeReason, ReadError> {
    match error {
        ReadError::Envelope(EnvelopeError::NotAnEnvelope { reason }) => {
            Ok(reason)
        }
        error @ (ReadError::Envelope(
            EnvelopeError::Json { .. }
            | EnvelopeError::DuplicateFields { .. }
            | EnvelopeError::UnknownFields { .. }
            | EnvelopeError::Version { .. },
        )
        | ReadError::UnsupportedVersion(_)
        | ReadError::Data { .. }
        | ReadError::Conversion(_)) => Err(error),
    }
}

fn latest_version<S: VersionSet>() -> u32 {
    *S::VERSIONS.last().expect(
        "the version set has at least one version, which \
         assert_version_set_well_formed established",
    )
}

/// This relies on `S::VERSIONS` being in ascending order.
/// (`assert_version_set_well_formed` establishes this.)
fn is_member<S: VersionSet>(version: u32) -> bool {
    S::VERSIONS.binary_search(&version).is_ok()
}

fn upgrade<S: VersionSet>(
    value: S,
    asked: u32,
    cx: &S::Context,
) -> Result<S::Latest, ConversionFailed> {
    let latest = latest_version::<S>();

    let mut value = value;
    let mut from = value.version();
    if !is_member::<S>(from) {
        panic!(
            "{}::parse was asked for version {asked} and returned a version \
             {from} value, but {from} is not in VERSIONS",
            type_name::<S>()
        );
    }
    if from < asked {
        panic!(
            "{}::parse was asked for version {asked} and returned a version \
             {from} value, which is older",
            type_name::<S>()
        );
    }

    loop {
        if from == latest {
            return match value.step(cx) {
                Step::Done(latest) => Ok(latest),
                Step::Next(next) => panic!(
                    "{}::step for version {from}, the last version in \
                     VERSIONS, returned Next naming version {}",
                    type_name::<S>(),
                    next.version()
                ),
                Step::Failed { to, source: _ } => panic!(
                    "{}::step for version {from}, the last version in \
                     VERSIONS, returned Failed naming version {to}",
                    type_name::<S>()
                ),
            };
        }

        match value.step(cx) {
            Step::Next(next) => {
                let to = next.version();
                assert_forward::<S>(from, to, "Next");
                value = next;
                from = to;
            }
            Step::Done(_) => panic!(
                "{}::step for version {from} returned Done, but the last \
                 version in VERSIONS is {latest}",
                type_name::<S>()
            ),
            Step::Failed { to, source } => {
                assert_forward::<S>(from, to, "Failed");
                return Err(ConversionFailed::new(from, to, source));
            }
        }
    }
}

fn assert_forward<S: VersionSet>(from: u32, to: u32, returned: &str) {
    if !is_member::<S>(to) {
        panic!(
            "{}::step for version {from} returned {returned} naming version \
             {to}, but {to} is not in VERSIONS",
            type_name::<S>()
        );
    }
    if to <= from {
        panic!(
            "{}::step for version {from} returned {returned} naming version \
             {to}, which is not newer",
            type_name::<S>()
        );
    }
}
