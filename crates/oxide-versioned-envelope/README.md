<!-- cargo-sync-rdme title [[ -->
# oxide-versioned-envelope
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme badge [[ -->
![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/oxide-versioned-envelope.svg?)
[![crates.io](https://img.shields.io/crates/v/oxide-versioned-envelope.svg?logo=rust)](https://crates.io/crates/oxide-versioned-envelope)
[![docs.rs](https://img.shields.io/docsrs/oxide-versioned-envelope.svg?logo=docs.rs)](https://docs.rs/oxide-versioned-envelope)
[![Rust: ^1.86.0](https://img.shields.io/badge/rust-^1.86.0-93450a.svg?logo=rust)](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field)
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme rustdoc [[ -->
Versioned envelopes for JSON data structures.

## Motivation

It is common in large systems to have versioned JSON blobs in transit or at
rest. In those cases, how can the side that is deserializing the blob know
which version it is dealing with? Broadly speaking, there are two ways to do
this:

1. Communicate the version out of band somehow. At Oxide we use this pattern
   for [Dropshot] API versions via an `api-version` header.
1. Store the version in an outer *envelope* object.

This crate implements the second option in a way that encodes best
practices.

## Format

The version is a `u32`, and is stored in a `version` field. The versioned
data itself is stored in a `data` field. For example:

````json
{
    "version": 1,
    "data": {
        "key": "value",
        // ...
    }
}
````

This format is carefully chosen to avoid double-encoding the JSON as either
a string or a `Vec<u8>`. But this means that deserializing data in a robust
manner with good error messages is slightly more complex, with separate
helpers for [reading][crate::read_json] and [writing][crate::WriteEnvelope].

## Usage

### As fixed versions

Let’s say you have a CLI option that returns data in a structured manner.
Clients are expected to know about the specific format version.

````rust
use oxide_versioned_envelope::{Versioned, WriteEnvelope, read_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ExecutionData {
    retries: u32,
}

// Let's say you have a value of this type.
let data = ExecutionData { retries: 3 };

// Define `Versioned` for the type.
impl Versioned for ExecutionData {
    const VERSION: u32 = 2;
}

// To serialize this value, use `WriteEnvelope`.
let bytes = serde_json::to_vec(&WriteEnvelope::new(&data))?;
assert_eq!(
    String::from_utf8(bytes.clone())?,
    r#"{"version":2,"data":{"retries":3}}"#
);

// To deserialize, use `read_json`. This verifies that
// the version is correct.
let read = read_json::<ExecutionData>(&bytes)?.into_value();
assert_eq!(read, data);
````

### As upgradable versions

Let’s say you have two versions of a serializable settings struct that’s
stored on disk:

````rust
use oxide_versioned_envelope::{
    Versioned, WriteEnvelope, read_json, read_json_or_untagged, version_set,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SettingsV1 {
    string_setting: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SettingsV2 {
    string_setting: String,
    u32_setting: Option<u32>,
}

// Whenever data is read, it should be converted into
// the latest version. Let's say there is a `From` impl
// to do this conversion.
impl From<SettingsV1> for SettingsV2 {
    fn from(value: SettingsV1) -> Self {
        Self { string_setting: value.string_setting, u32_setting: None }
    }
}

// Let's say you have this value that you serialize as
// V1.
let settings = SettingsV1 { string_setting: "hello".to_string() };

// Implement `Versioned` for each version.
impl Versioned for SettingsV1 {
    const VERSION: u32 = 1;
}

impl Versioned for SettingsV2 {
    const VERSION: u32 = 2;
}

// To serialize this value, use `WriteEnvelope`.
let bytes = serde_json::to_vec(&WriteEnvelope::new(&settings))?;
assert_eq!(
    String::from_utf8(bytes.clone())?,
    r#"{"version":1,"data":{"string_setting":"hello"}}"#
);

// To deserialize this version and convert it to the latest version,
// use `version_set!` to define an enum which represents all the
// versions, oldest first. The macro checks at compile time that every
// step goes to a newer version.
version_set! {
    enum Settings -> SettingsV2 {
        V1(SettingsV1) => V2,
        V2(SettingsV2),
    }
}

// Now let's read this value and automatically upgrade it to
// the latest version.
let expected =
    SettingsV2 { string_setting: "hello".to_string(), u32_setting: None };
let read = read_json::<Settings>(&bytes)?;
// The document was written as version 1, so it should be rewritten as
// version 2.
assert!(read.needs_rewrite());
let settings: SettingsV2 = read.into_value();
assert_eq!(settings, expected);

// ---

// Sometimes you might have a version on disk that predates the
// envelope. In that case, `read_json_or_untagged` tries
// deserializing as an envelope, otherwise as versions in
// descending order.
let legacy = serde_json::to_vec(&settings)?;
let settings: SettingsV2 =
    read_json_or_untagged::<Settings>(&legacy)?.into_value();
assert_eq!(settings, expected);
````

For more read helpers, including documentation for when to use them, see
[`read_json`].

[Dropshot]: https://docs.rs/dropshot
[crate::read_json]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json.html "fn oxide_versioned_envelope::read::imp::read_json"
[crate::WriteEnvelope]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/write/imp/struct.WriteEnvelope.html "struct oxide_versioned_envelope::write::imp::WriteEnvelope"
[`read_json`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json.html "fn oxide_versioned_envelope::read::imp::read_json"
<!-- cargo-sync-rdme ]] -->

## License

This project is available under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT license](LICENSE-MIT).
