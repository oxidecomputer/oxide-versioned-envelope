use crate::errors::NotAnEnvelopeReason;

/// A successfully read value.
///
/// Returned by the `read_*` methods.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOutput<T> {
    value: T,
    origin: Origin,
    latest: u32,
}

impl<T> ReadOutput<T> {
    pub(crate) fn new(value: T, origin: Origin, latest: u32) -> Self {
        Self { value, origin, latest }
    }

    /// Returns a reference to the deserialized value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consumes self, returning the value.
    pub fn into_value(self) -> T {
        self.value
    }

    /// Returns the place from which the value was read.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Consumes self, returning the value and origin.
    pub fn into_parts(self) -> (T, Origin) {
        (self.value, self.origin)
    }

    /// Returns true if the value needs to be rewritten.
    ///
    /// This returns true if any of the following are true:
    ///
    /// * The value was in a version older than the latest.
    /// * This was an untagged or fallback value.
    pub fn needs_rewrite(&self) -> bool {
        match self.origin {
            Origin::Envelope { version } => version < self.latest,
            Origin::Untagged { .. } | Origin::Fallback { .. } => true,
        }
    }
}

/// The method through which a versioned JSON blob was read.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Origin {
    /// The data was in a versioned envelope.
    ///
    /// For [`read_json`](crate::read_json) and
    /// [`read_json_with`](crate::read_json_with), this is the only variant
    /// possible.
    Envelope {
        /// The version of data read.
        version: u32,
    },

    /// The data was not in a versioned envelope, and one of the `untagged`
    /// functions was used to read it.
    Untagged {
        /// The version of data for which deserializing was successful.
        version: u32,
    },

    /// The data was not in a versioned envelope, and one of the `or_else`
    /// functions was used to read it.
    Fallback {
        /// The reason the data was not detected as a versioned envelope.
        not_an_envelope: NotAnEnvelopeReason,
    },
}
