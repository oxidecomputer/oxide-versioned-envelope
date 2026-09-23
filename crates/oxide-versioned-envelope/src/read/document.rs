use super::validate::Validated;
use crate::{
    errors::NonObjectKind,
    wire::{DATA_FIELD, VERSION_FIELD},
};
use serde_core::de::{
    Deserialize, DeserializeSeed, Deserializer, Error, IgnoredAny, MapAccess,
    SeqAccess, Visitor,
};
use std::{collections::BTreeSet, fmt};

/// The first step towards parsing a versioned envelope.
///
/// This detects the presence or absence of `data` and `version` fields, without
/// trying to deserialize them yet.
pub(super) enum Document {
    Object(ObjectFields),
    NonObject(NonObjectKind),
}

impl Document {
    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

pub(super) struct ObjectFields {
    pub(super) version: Field,
    pub(super) data: Field,
    pub(super) unknown: BTreeSet<String>,
}

pub(super) enum Field {
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

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        // Use `deserialize_any` rather than `deserialize_map` here so that we
        // get more structured data about the error cases. (See DocumentVisitor
        // below for how incorrect documents are reported.)
        deserializer.deserialize_any(DocumentVisitor)
    }
}

struct DocumentVisitor;

impl<'de> Visitor<'de> for DocumentVisitor {
    type Value = Document;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E: serde_core::de::Error>(self) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::Null))
    }

    fn visit_bool<E: serde_core::de::Error>(
        self,
        _value: bool,
    ) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::Bool))
    }

    fn visit_i64<E: serde_core::de::Error>(
        self,
        _value: i64,
    ) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::Number))
    }

    fn visit_u64<E: serde_core::de::Error>(
        self,
        _value: u64,
    ) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::Number))
    }

    fn visit_f64<E: serde_core::de::Error>(
        self,
        _value: f64,
    ) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::Number))
    }

    fn visit_str<E: serde_core::de::Error>(
        self,
        _value: &str,
    ) -> Result<Self::Value, E> {
        Ok(Document::NonObject(NonObjectKind::String))
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<Self::Value, A::Error> {
        // Drain the sequence and report that this isn't of the right shape.
        while seq.next_element::<Validated>()?.is_some() {}
        Ok(Document::NonObject(NonObjectKind::Array))
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

        Ok(Document::Object(fields))
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
            DATA_FIELD => Key::Data,
            _ => Key::Other(value.to_owned()),
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum EnvelopeField {
    Version,
    Data,
}

impl EnvelopeField {
    fn name(self) -> &'static str {
        match self {
            Self::Version => VERSION_FIELD,
            Self::Data => DATA_FIELD,
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
pub(super) fn deserialize_field<'de, T: DeserializeSeed<'de>>(
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
        let mut seed = Some(seed);
        let mut value = None;

        while let Some(key) = map.next_key::<Key>()? {
            if !field.matches(&key) {
                map.next_value::<IgnoredAny>()?;
                continue;
            }
            let seed = seed
                .take()
                .ok_or_else(|| A::Error::duplicate_field(field.name()))?;
            value = Some(map.next_value_seed(seed)?);
        }

        value.ok_or_else(|| A::Error::missing_field(field.name()))
    }
}
