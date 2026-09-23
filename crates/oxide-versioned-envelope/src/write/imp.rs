use crate::{
    versioned::Versioned,
    wire::{DATA_FIELD, VERSION_FIELD},
};
use serde_core::{Serialize, Serializer, ser::SerializeStruct};

/// An envelope for serializing [`Versioned`] data.
///
/// This type forms the write side of this crate. To use it:
///
/// * Implement [`Versioned`] for a type.
/// * Create a `WriteEnvelope`.
/// * Serialize it via [`serde_json`].
///
/// # Implementations
///
/// This type deliberately does not implement deserialization. To deserialize
/// data, use the `read_*` functions in this crate.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{Versioned, WriteEnvelope};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Heartbeat {
///     uptime_secs: u64,
/// }
///
/// // Implement `Versioned` for your type.
/// impl Versioned for Heartbeat {
///     const VERSION: u32 = 1;
/// }
///
/// // Let's say you'd like to serialize this value.
/// let heartbeat = Heartbeat { uptime_secs: 42 };
///
/// // Create a `WriteEnvelope` and serialize it.
/// let envelope = WriteEnvelope::new(&heartbeat);
/// let json = serde_json::to_string(&envelope)?;
/// assert_eq!(json, r#"{"version":1,"data":{"uptime_secs":42}}"#);
/// # Ok(()) }
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WriteEnvelope<T> {
    version: u32,
    data: T,
}

impl<T: Versioned> WriteEnvelope<T> {
    /// Creates a new `WriteEnvelope` with the given data, and the version set
    /// to [`Versioned::VERSION`].
    pub fn new(data: T) -> Self {
        Self { version: T::VERSION, data }
    }
}

impl<T: Serialize> Serialize for WriteEnvelope<T> {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut envelope = serializer.serialize_struct("WriteEnvelope", 2)?;
        envelope.serialize_field(VERSION_FIELD, &self.version)?;
        envelope.serialize_field(DATA_FIELD, &self.data)?;
        envelope.end()
    }
}
