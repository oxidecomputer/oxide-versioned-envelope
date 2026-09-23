use serde_core::de::{
    Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor,
};
use std::fmt;

/// Ensures the bytes are valid JSON.
///
/// The goal is to match exactly what deserializing to `serde_json::Value` would
/// produce, without deserializing to that type (which is lossy in a few
/// different ways; see the doc comment at the top of `./imp.rs`).
pub(super) fn validate_json(bytes: &[u8]) -> Result<(), serde_json::Error> {
    serde_json::from_slice::<Validated>(bytes).map(|Validated| ())
}

pub(super) struct Validated;

impl<'de> Deserialize<'de> for Validated {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        // What we care about for this pass is not that the data is necessarily
        // in the right format, but rather only that it is valid JSON.
        deserializer.deserialize_any(ValidatedVisitor)
    }
}

struct ValidatedVisitor;

impl<'de> Visitor<'de> for ValidatedVisitor {
    type Value = Validated;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_bool<E: Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_i64<E: Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_u64<E: Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_f64<E: Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_str<E: Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(Validated)
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<Self::Value, A::Error> {
        while seq.next_element::<Validated>()?.is_some() {}
        Ok(Validated)
    }

    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> Result<Self::Value, A::Error> {
        while map.next_key::<Validated>()?.is_some() {
            map.next_value::<Validated>()?;
        }
        Ok(Validated)
    }
}
