use super::validate::{Validated, validate_json};
use crate::{
    errors::{
        EnvelopeError, EnvelopeFieldSet, NonObjectKind, NotAnEnvelopeReason,
    },
    wire::{VERSION_FIELD, VERSIONED_DATA_FIELD},
};
use serde_core::de::{
    Deserialize, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor,
};
use std::{collections::BTreeSet, fmt, marker::PhantomData};

/// A document with exactly one `version` field that's a `u32`, exactly one
/// `versioned_data` field, and nothing else.
pub(super) struct Envelope<'de> {
    bytes: &'de [u8],
    version: u32,
}

impl<'de> Envelope<'de> {
    pub(super) fn parse(bytes: &'de [u8]) -> Result<Self, EnvelopeError> {
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

        let version = deserialize_field(
            bytes,
            EnvelopeField::Version,
            PhantomData::<u32>,
        )
        .map_err(|source| EnvelopeError::Version { source })?;
        Ok(Self { bytes, version })
    }

    pub(super) fn version(&self) -> u32 {
        self.version
    }

    pub(super) fn data<T: DeserializeSeed<'de>>(
        &self,
        seed: T,
    ) -> Result<T::Value, serde_json::Error> {
        deserialize_field(self.bytes, EnvelopeField::Data, seed)
    }
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

/// The first step towards parsing a versioned envelope.
///
/// This detects the presence or absence of `versioned_data` and `version`
/// fields, without trying to deserialize them yet.
enum Document {
    Object(ObjectFields),
    NonObject(NonObjectKind),
}

impl Document {
    fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        // Don't classify the root with deserialize_any here. If
        // serde_json/arbitrary_precision is turned on anywhere in the graph,
        // deserialize_any reports floats and large integers as maps, producing
        // worse error messages.
        let first = first_non_whitespace(bytes);
        if first == Some(b'{') {
            return serde_json::from_slice(bytes).map(Document::Object);
        }

        validate_json(bytes)?;
        let kind = match first {
            Some(b'n') => NonObjectKind::Null,
            Some(b't' | b'f') => NonObjectKind::Bool,
            Some(b'-' | b'0'..=b'9') => NonObjectKind::Number,
            Some(b'"') => NonObjectKind::String,
            Some(b'[') => NonObjectKind::Array,
            other => panic!(
                "valid JSON that is not an object starts with a null, bool, \
                 number, string, or array byte, but it started with {other:?}"
            ),
        };
        Ok(Document::NonObject(kind))
    }
}

/// Whitespace bytes allowed by RFC 8259. (`serde_json` uses the same definition
/// of whitespace.)
const JSON_WHITESPACE: &[u8] = b" \t\n\r";

fn first_non_whitespace(bytes: &[u8]) -> Option<u8> {
    bytes.iter().copied().find(|byte| !JSON_WHITESPACE.contains(byte))
}

struct ObjectFields {
    version: Field,
    data: Field,
    unknown: BTreeSet<String>,
}

enum Field {
    Absent,
    Present,
    Duplicated,
}

impl Field {
    fn set(&mut self) {
        match self {
            Self::Absent => *self = Self::Present,
            Self::Present | Self::Duplicated => *self = Self::Duplicated,
        }
    }
}

impl<'de> Deserialize<'de> for ObjectFields {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ObjectFieldsVisitor)
    }
}

struct ObjectFieldsVisitor;

impl<'de> Visitor<'de> for ObjectFieldsVisitor {
    type Value = ObjectFields;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> Result<Self::Value, A::Error> {
        let mut fields = ObjectFields {
            version: Field::Absent,
            data: Field::Absent,
            unknown: BTreeSet::new(),
        };

        while let Some(key) = map.next_key::<Key>()? {
            map.next_value::<Validated>()?;
            // This pass records which fields are present. We parse the document
            // again to actually deserialize it.
            match key {
                Key::Version => fields.version.set(),
                Key::Data => fields.data.set(),
                Key::Other(name) => {
                    fields.unknown.insert(name);
                }
            }
        }

        Ok(fields)
    }
}

enum Key {
    Version,
    Data,
    Other(String),
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        deserializer.deserialize_identifier(KeyVisitor)
    }
}

struct KeyVisitor;

impl Visitor<'_> for KeyVisitor {
    type Value = Key;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object key")
    }

    fn visit_str<E: serde_core::de::Error>(
        self,
        value: &str,
    ) -> Result<Self::Value, E> {
        Ok(match value {
            VERSION_FIELD => Key::Version,
            VERSIONED_DATA_FIELD => Key::Data,
            _ => Key::Other(value.to_owned()),
        })
    }
}

#[derive(Clone, Copy)]
enum EnvelopeField {
    Version,
    Data,
}

impl EnvelopeField {
    fn name(self) -> &'static str {
        match self {
            Self::Version => VERSION_FIELD,
            Self::Data => VERSIONED_DATA_FIELD,
        }
    }

    fn matches(self, key: &Key) -> bool {
        match (self, key) {
            (Self::Version, Key::Version) | (Self::Data, Key::Data) => true,
            (Self::Version, Key::Data | Key::Other(_))
            | (Self::Data, Key::Version | Key::Other(_)) => false,
        }
    }
}

/// Deserializes a single field from the document.
///
/// Deserializing a single field out of the whole document, rather than out of a
/// slice of it, makes serde_json report error positions relative to the whole
/// document.
fn deserialize_field<'de, T: DeserializeSeed<'de>>(
    bytes: &'de [u8],
    field: EnvelopeField,
    seed: T,
) -> Result<T::Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = FieldSeed { field, seed }.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

struct FieldSeed<T> {
    field: EnvelopeField,
    seed: T,
}

impl<'de, T: DeserializeSeed<'de>> DeserializeSeed<'de> for FieldSeed<T> {
    type Value = T::Value;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de, T: DeserializeSeed<'de>> Visitor<'de> for FieldSeed<T> {
    type Value = T::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "an object with a `{}` field", self.field.name())
    }

    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> Result<Self::Value, A::Error> {
        let Self { field, seed } = self;

        while let Some(key) = map.next_key::<Key>()? {
            if !field.matches(&key) {
                map.next_value::<IgnoredAny>()?;
                continue;
            }
            let value = map.next_value_seed(seed)?;
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            return Ok(value);
        }

        panic!(
            "the envelope has a `{}` field, which check_fields established",
            field.name()
        );
    }
}
