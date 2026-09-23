#![cfg_attr(doc_cfg, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Versioned envelopes for JSON data structures.
//!
//! # Motivation
//!
//! It is common in large systems to have versioned JSON blobs in transit or at
//! rest. In those cases, how can the side that is deserializing the blob know
//! which version it is dealing with? Broadly speaking, there are three ways to
//! do this:
//!
//! 1. Communicate the version out of band somehow. At Oxide we use this pattern
//!    for [Dropshot] API versions via an `api-version` header.
//! 2. Store the version in an outer _envelope_ object.
//! 3. Try deserializing as the latest version, then attempt to deserialize as
//!    all prior versions walking back from the latest. We call this pattern
//!    _untagged deserialization_.
//!
//! This crate implements the second option in a way that encodes best
//! practices, along with a way to migrate from the third option.
//!
//! # Features
//!
//! * A simple format with `version` and `data` fields. See _Format_ below.
//! * Writing with a version attached. See [`WriteEnvelope`].
//! * Reading a fixed version, or upgrading from older versions on read. See
//!   [`read_json`] and _Usage_ below.
//! * For pre-envelope data, falling back to
//!   [untagged deserialization](read_json_or_untagged)
//!   or to [a custom callback](read_json_or_else).
//! * Knowing whether to rewrite serialized data, with
//!   [`ReadOutput::needs_rewrite`].
//! * JSON Schema support with the `schemars08` feature.
//! * A focus on correctness and good error handling. (The envelope checks
//!   are careful and strict, and this crate does not use [`serde_json::Value`]
//!   when reading.)
//! * No proc macros, minimal dependencies: only [`serde_core`] and
//!   [`serde_json`] by default.
//!
//! # Format
//!
//! The version is a `u32`, and is stored in a `version` field. The versioned
//! data itself is stored in a `data` field. For example:
//!
//! ```json
//! {
//!     "version": 1,
//!     "data": {
//!         "key": "value",
//!         // ...
//!     }
//! }
//! ```
//!
//! This format is carefully chosen to avoid double-encoding the JSON as either
//! a string or a `Vec<u8>`. But this means that deserializing data in a robust
//! manner with good error messages is slightly more complex, with separate
//! helpers for [reading](crate::read_json) and [writing](crate::WriteEnvelope).
//!
//! # Usage
//!
//! ## As fixed versions
//!
//! Let's say you have a CLI option that returns data in a structured manner.
//! Clients are expected to know about the specific format version.
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use oxide_versioned_envelope::{Versioned, WriteEnvelope, read_json};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
//! struct ExecutionData {
//!     retries: u32,
//! }
//!
//! // Let's say you have a value of this type.
//! let data = ExecutionData { retries: 3 };
//!
//! // Define `Versioned` for the type.
//! impl Versioned for ExecutionData {
//!     const VERSION: u32 = 2;
//! }
//!
//! // To serialize this value, use `WriteEnvelope`.
//! let bytes = serde_json::to_vec(&WriteEnvelope::new(&data))?;
//! assert_eq!(
//!     String::from_utf8(bytes.clone())?,
//!     r#"{"version":2,"data":{"retries":3}}"#
//! );
//!
//! // To deserialize, use `read_json`. This verifies that
//! // the version is correct.
//! //
//! // On success, read_json (and all the other read methods) return
//! // a `ReadOutput`, which contains the requested data and information
//! // about where it comes from, whether it needs a rewrite, etc.
//! // To extract the inner value, call `into_value`.
//! let read = read_json::<ExecutionData>(&bytes)?.into_value();
//! assert_eq!(read, data);
//! # Ok(()) }
//! ```
//!
//! ## As upgradable versions
//!
//! Let's say you have two versions of a serializable settings struct that's
//! stored on disk:
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use oxide_versioned_envelope::{
//!     Versioned, WriteEnvelope, read_json, read_json_or_untagged, version_set,
//! };
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
//! struct SettingsV1 {
//!     string_setting: String,
//! }
//!
//! #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
//! struct SettingsV2 {
//!     string_setting: String,
//!     u32_setting: Option<u32>,
//! }
//!
//! // Whenever data is read, it should be converted into
//! // the latest version. Let's say there is a `From` impl
//! // to do this conversion.
//! impl From<SettingsV1> for SettingsV2 {
//!     fn from(value: SettingsV1) -> Self {
//!         Self { string_setting: value.string_setting, u32_setting: None }
//!     }
//! }
//!
//! // Let's say you have this value that you serialize as
//! // V1.
//! let settings = SettingsV1 { string_setting: "hello".to_string() };
//!
//! // Implement `Versioned` for each version.
//! impl Versioned for SettingsV1 {
//!     const VERSION: u32 = 1;
//! }
//!
//! impl Versioned for SettingsV2 {
//!     const VERSION: u32 = 2;
//! }
//!
//! // To serialize this value, use `WriteEnvelope`.
//! let bytes = serde_json::to_vec(&WriteEnvelope::new(&settings))?;
//! assert_eq!(
//!     String::from_utf8(bytes.clone())?,
//!     r#"{"version":1,"data":{"string_setting":"hello"}}"#
//! );
//!
//! // To deserialize this version and convert it to the latest version,
//! // use `version_set!` to define an enum which represents all the
//! // versions, oldest first. The macro checks at compile time that every
//! // step goes to a newer version.
//! version_set! {
//!     enum Settings -> SettingsV2 {
//!         V1(SettingsV1) => V2,
//!         V2(SettingsV2),
//!     }
//! }
//!
//! // Now let's read this value and automatically upgrade it to
//! // the latest version.
//! let expected =
//!     SettingsV2 { string_setting: "hello".to_string(), u32_setting: None };
//! let read = read_json::<Settings>(&bytes)?;
//! // The document was written as version 1, so it should be rewritten as
//! // version 2.
//! assert!(read.needs_rewrite());
//! let settings: SettingsV2 = read.into_value();
//! assert_eq!(settings, expected);
//!
//! // ---
//!
//! // Sometimes you might have a version on disk that predates the
//! // envelope. In that case, `read_json_or_untagged` tries
//! // deserializing as an envelope, otherwise as versions in
//! // descending order.
//! let legacy = serde_json::to_vec(&settings)?;
//! let settings: SettingsV2 =
//!     read_json_or_untagged::<Settings>(&legacy)?.into_value();
//! assert_eq!(settings, expected);
//! # Ok(()) }
//! ```
//!
//! For more read helpers, including documentation for when to use them, see
//! [`read_json`].
//!
//! # Cargo features
//!
//! * `schemars08`: Enables JSON schema generation for `WriteEnvelope`.
//!   *Not enabled by default*.
//!
//! # Minimum supported Rust version (MSRV)
//!
//! This crate's MSRV is **Rust 1.86**. In general we aim for 6 months of Rust
//! compatibility.
//!
//! [Dropshot]: https://docs.rs/dropshot

pub mod errors;
mod read;
mod versioned;
mod wire;
mod write;

pub use read::{
    Origin, ReadOutput, read_json, read_json_or_else, read_json_or_else_with,
    read_json_or_untagged, read_json_or_untagged_with, read_json_with,
    read_untagged_json, read_untagged_json_with,
};
pub use versioned::{
    Step, VersionSet, Versioned, assert_version_set_well_formed,
};
pub use write::WriteEnvelope;

/// Paths used by macros. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use serde_core;
}
