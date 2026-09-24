//! Integration tests for reading values.

use hegel::{
    TestCase,
    generators::{self as gs, Generator},
};
use integration_tests::{
    AnyPayload, SETTINGS_VERSIONS, Settings, SettingsV1, SettingsV4,
};
use oxide_versioned_envelope::{
    Origin, Step, VersionSet, Versioned, WriteEnvelope,
    errors::{
        EnvelopeError, EnvelopeFieldSet, NonObjectKind, NotAnEnvelopeReason,
        ReadError, ReadOrFallbackError, UnsupportedVersionKind, UntaggedError,
    },
    read_json, read_json_or_else, read_json_or_untagged, read_untagged_json,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json, value::RawValue};
use std::{collections::BTreeSet, convert::Infallible, error::Error};

#[hegel::test]
fn written_payload_roundtrip(tc: TestCase) {
    let payload = tc.draw(gs::default::<AnyPayload>());
    let version = payload.version();
    let bytes = payload.write();

    let read = read_json::<Settings>(&bytes);
    // Try upgrading the input payload ourselves.
    match payload.upgrade() {
        Ok(expected) => {
            let read = read.expect("the payload upgrades");
            assert_eq!(read.origin(), Origin::Envelope { version });
            assert_eq!(read.needs_rewrite(), version < 4);
            assert_eq!(read.into_value(), expected);
        }
        Err(_) => {
            let Err(ReadError::Conversion(failure)) = read else {
                panic!("expected a conversion failure, got {read:?}");
            };
            assert_eq!((failure.from_version(), failure.to_version()), (2, 4));
        }
    }
}

struct CapturedData {
    version: u32,
    data: String,
}

impl VersionSet for CapturedData {
    type Latest = String;
    type Context = ();
    const VERSIONS: &'static [u32] = &SETTINGS_VERSIONS;

    fn parse<'de, D: Deserializer<'de>>(
        version: u32,
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let data = Box::<RawValue>::deserialize(deserializer)?;
        Ok(Self { version, data: data.get().to_owned() })
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn step(self, _cx: &()) -> Step<Self> {
        match SETTINGS_VERSIONS.iter().find(|&&next| next > self.version) {
            Some(&next) => Step::Next(Self { version: next, ..self }),
            None => Step::Done(self.data),
        }
    }
}

/// Generator for a `serde_json::Value` of an interesting shape.
#[hegel::composite]
fn values(tc: &TestCase, depth: u32) -> Value {
    let key = gs::sampled_from(&["version", "data", "Version", "other"][..])
        .map(str::to_owned);
    let leaf = hegel::one_of!(
        gs::just(Value::Null),
        gs::booleans().map(Value::Bool),
        gs::sampled_from(&[0, 1, 2, 3, 4, 5][..]).map(|n| json!(n)),
        gs::integers::<i64>().map(|n| json!(n)),
        gs::sampled_from(&[-0.0, 2.0, 0.5][..]).map(|n| json!(n)),
        gs::text().max_size(4).map(Value::String),
    );
    if depth == 0 || tc.draw(gs::booleans()) {
        return tc.draw(leaf.print_as_debug());
    }
    if tc.draw(gs::weighted_booleans(0.2)) {
        return Value::Array(
            tc.draw(
                gs::vecs(values(depth - 1).print_as_debug())
                    .max_size(2)
                    .print_as_debug(),
            ),
        );
    }
    let members = tc.draw(
        gs::vecs(hegel::compose!(|tc| {
            (tc.draw(&key), tc.draw(values(depth - 1).print_as_debug()))
        }))
        .max_size(4)
        .print_as_debug(),
    );
    Value::Object(members.into_iter().collect::<Map<_, _>>())
}

#[derive(Debug, Eq, PartialEq)]
struct EnvelopeContents {
    version: u32,
    data: Value,
}

#[derive(Debug, Eq, PartialEq)]
enum Refusal {
    NotAnEnvelope(NotAnEnvelopeReason),
    UnknownFields(BTreeSet<String>),
    BadVersion,
    Unsupported(u32),
}

/// Determines the value that `read_json::<CapturedData>(text)` should return by
/// examining the document.
fn model_read(document: &Value) -> Result<EnvelopeContents, Refusal> {
    let kind = match document {
        Value::Object(_) => None,
        Value::Null => Some(NonObjectKind::Null),
        Value::Bool(_) => Some(NonObjectKind::Bool),
        Value::Number(_) => Some(NonObjectKind::Number),
        Value::String(_) => Some(NonObjectKind::String),
        Value::Array(_) => Some(NonObjectKind::Array),
    };
    if let Some(kind) = kind {
        return Err(Refusal::NotAnEnvelope(
            NotAnEnvelopeReason::NonObjectRoot { kind },
        ));
    }
    let (version, data) = match (document.get("version"), document.get("data"))
    {
        (Some(version), Some(data)) => (version, data),
        (None, None) => return Err(missing(EnvelopeFieldSet::VersionAndData)),
        (None, Some(_)) => return Err(missing(EnvelopeFieldSet::Version)),
        (Some(_), None) => return Err(missing(EnvelopeFieldSet::Data)),
    };
    let unknown: BTreeSet<String> = document
        .as_object()
        .expect("the root is an object")
        .keys()
        .filter(|key| *key != "version" && *key != "data")
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(Refusal::UnknownFields(unknown));
    }
    let version = version
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
        .ok_or(Refusal::BadVersion)?;
    if !SETTINGS_VERSIONS.contains(&version) {
        return Err(Refusal::Unsupported(version));
    }
    Ok(EnvelopeContents { version, data: data.clone() })
}

fn missing(fields: EnvelopeFieldSet) -> Refusal {
    Refusal::NotAnEnvelope(NotAnEnvelopeReason::Missing { fields })
}

fn actual_read(bytes: &[u8]) -> Result<EnvelopeContents, Refusal> {
    match read_json::<CapturedData>(bytes) {
        Ok(read) => match read.origin() {
            Origin::Envelope { version } => {
                let data = serde_json::from_str(&read.into_value())
                    .expect("the captured data is JSON");
                Ok(EnvelopeContents { version, data })
            }
            origin @ (Origin::Untagged { .. } | Origin::Fallback { .. }) => {
                panic!("read_json reads only envelopes, not {origin:?}")
            }
        },
        Err(ReadError::Envelope(EnvelopeError::NotAnEnvelope { reason })) => {
            Err(Refusal::NotAnEnvelope(reason))
        }
        Err(ReadError::Envelope(EnvelopeError::UnknownFields { names })) => {
            Err(Refusal::UnknownFields(names))
        }
        Err(ReadError::Envelope(EnvelopeError::Version { .. })) => {
            Err(Refusal::BadVersion)
        }
        Err(ReadError::UnsupportedVersion(unsupported)) => {
            Err(Refusal::Unsupported(unsupported.found()))
        }
        Err(
            error @ (ReadError::Envelope(
                EnvelopeError::Json { .. }
                | EnvelopeError::DuplicateFields { .. },
            )
            | ReadError::Data { .. }
            | ReadError::Conversion(_)),
        ) => panic!("a document serde_json wrote cannot fail with {error:?}"),
    }
}

/// PBTs for enveloped data.
#[hegel::test]
fn envelopes_are_read(tc: TestCase) {
    let document = tc.draw(values(3).print_as_debug());
    let bytes = if tc.draw(gs::booleans()) {
        serde_json::to_vec(&document)
    } else {
        serde_json::to_vec_pretty(&document)
    }
    .expect("the document serialized to JSON");
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(actual_read(&bytes), model_read(&document), "{text}");
}

/// Tests that we do not use the somewhat lossy `serde_json::Value` internally.
#[test]
fn does_not_use_serde_json_value() {
    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Wide {
        value: u128,
    }

    impl Versioned for Wide {
        const VERSION: u32 = 1;
    }

    // This wouldn't roundtrip through `serde_json::Value` since it's too large
    // for that.
    let bytes = br#"{"version":1,"data":{"value":18446744073709551616}}"#;
    let read = read_json::<Wide>(bytes).expect("a version 1 payload");
    assert_eq!(read.into_value(), Wide { value: 1 << 64 });
}

/// Tests unsupported versions.
#[test]
fn unsupported_versions() {
    for (found, kind) in [
        (0, UnsupportedVersionKind::Older { oldest: 1 }),
        (3, UnsupportedVersionKind::Gap),
        (5, UnsupportedVersionKind::Newer { latest: 4 }),
    ] {
        let bytes = format!(r#"{{"version":{found},"data":{{}}}}"#);
        let Err(ReadError::UnsupportedVersion(unsupported)) =
            read_json::<Settings>(bytes.as_bytes())
        else {
            panic!("version {found} is unsupported");
        };
        assert_eq!(unsupported.kind(), kind, "version {found}");
        assert_eq!(unsupported.supported(), SETTINGS_VERSIONS);
    }
}

/// Tests that untagged reads work backwards from the latest version.
#[test]
fn untagged_reads() {
    let v4 = serde_json::to_vec(&SettingsV4 { value: 1, label: "a".into() })
        .expect("serialized");
    let read =
        read_untagged_json::<Settings>(&v4).expect("a version 4 payload");
    assert_eq!(read.origin(), Origin::Untagged { version: 4 });
    assert!(read.needs_rewrite());

    let v1 = serde_json::to_vec(&SettingsV1 { value: 1 }).expect("serialized");
    let Err(UntaggedError::Conversion(failure)) =
        read_untagged_json::<Settings>(&v1)
    else {
        panic!("a label-less payload fails to upgrade");
    };
    assert_eq!((failure.from_version(), failure.to_version()), (2, 4));

    let Err(UntaggedError::NoMatchingVersion(none)) =
        read_untagged_json::<Settings>(b"7")
    else {
        panic!("no version parses a number");
    };
    let attempts: Vec<u32> =
        none.attempts().iter().map(|attempt| attempt.version()).collect();
    assert_eq!(attempts, [4, 2, 1]);
}

/// Tests that the fallback runs only when a document is not an envelope.
#[test]
fn fallback_runs_only_for_non_envelopes() {
    let legacy = SettingsV4 { value: 9, label: "legacy".into() };
    let cases: [(&[u8], Option<NotAnEnvelopeReason>); 6] = [
        (
            br#"{"value":1}"#,
            Some(NotAnEnvelopeReason::Missing {
                fields: EnvelopeFieldSet::VersionAndData,
            }),
        ),
        (
            b"true",
            Some(NotAnEnvelopeReason::NonObjectRoot {
                kind: NonObjectKind::Bool,
            }),
        ),
        (br#"{"version":4,"data":{"value":1,"label":"a"}}"#, None),
        (br#"{"version":4,"data":{"value":"one"}}"#, None),
        (br#"{"version":"4","data":{}}"#, None),
        (br#"{"version":4,"data":{"value":1,"label":"a"},"extra":1}"#, None),
    ];
    for (bytes, reason) in cases {
        let mut handed = Vec::new();
        let read = read_json_or_else::<Settings, _, Infallible>(bytes, |r| {
            handed.push(r);
            Ok(legacy.clone())
        });
        assert_eq!(
            handed,
            Vec::from_iter(reason),
            "{}",
            String::from_utf8_lossy(bytes)
        );
        match reason {
            Some(reason) => {
                let read = read.expect("the fallback succeeds");
                assert_eq!(
                    read.origin(),
                    Origin::Fallback { not_an_envelope: reason }
                );
                assert!(read.needs_rewrite());
                assert_eq!(read.into_value(), legacy);
            }
            None => assert_eq!(
                read.map(|read| read.into_value()).map_err(|e| e.to_string()),
                read_json::<Settings>(bytes)
                    .map(|read| read.into_value())
                    .map_err(|e| e.to_string()),
            ),
        }
    }

    let read = read_json_or_untagged::<Settings>(br#"{"value":1,"label":"a"}"#)
        .expect("an untagged version 4 payload");
    assert_eq!(read.origin(), Origin::Untagged { version: 4 });
    let Err(ReadOrFallbackError::Read(ReadError::Data { version: 4, .. })) =
        read_json_or_untagged::<Settings>(br#"{"version":4,"data":{}}"#)
    else {
        panic!("a damaged envelope is not read untagged");
    };
}

#[test]
fn escaped_keys_are_accepted() {
    let bytes = br#"{"\u0076ersion":4,"d\u0061ta":{"value":1,"label":"a"}}"#;
    let read = read_json::<Settings>(bytes).expect("an envelope");
    assert_eq!(read.into_value(), SettingsV4 { value: 1, label: "a".into() });
}

#[test]
fn written_envelopes_are_exact() {
    let payload = SettingsV4 { value: 1, label: "a".into() };
    assert_eq!(
        serde_json::to_string(&WriteEnvelope::new(&payload))
            .expect("serialized"),
        r#"{"version":4,"data":{"value":1,"label":"a"}}"#,
    );
}

#[test]
fn fallback_errors_need_not_implement_error() {
    // Box<dyn Error + Send + Sync> doesn't implement Error, and fallbacks must
    // still be able to return it (as with anyhow::Error).
    let read = read_json_or_else::<Settings, _, Box<dyn Error + Send + Sync>>(
        b"true",
        |_| Err("the legacy reader refused".into()),
    );
    let Err(ReadOrFallbackError::Fallback { source, .. }) = read else {
        panic!("the fallback refuses the document");
    };
    assert_eq!(source.to_string(), "the legacy reader refused");
}

#[test]
fn error_positions_are_in_the_document() {
    let bad_version = b"{\n  \"version\": \"4\",\n  \"data\": {}\n}";
    let Err(ReadError::Envelope(EnvelopeError::Version { source })) =
        read_json::<Settings>(bad_version)
    else {
        panic!("the version is a string");
    };
    assert_eq!((source.line(), source.column()), (2, 16));

    let bad_data = b"{\n  \"version\": 2,\n  \"data\": {\n    \"value\": \"seven\"\n  }\n}";
    let Err(ReadError::Data { source, .. }) = read_json::<Settings>(bad_data)
    else {
        panic!("the value is a string");
    };
    assert_eq!((source.line(), source.column()), (4, 20));

    let Err(UntaggedError::NoMatchingVersion(none)) =
        read_untagged_json::<Settings>(b"\n\n  7")
    else {
        panic!("no version parses a number");
    };
    for attempt in none.attempts() {
        let error = attempt.error();
        assert_eq!((error.line(), error.column()), (3, 3));
    }
}
