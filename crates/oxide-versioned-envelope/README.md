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

It is common in large systems to have versioned JSON payloads in transit or
at rest. In those cases, how can the side that is deserializing the blob
know which version it is dealing with? Broadly speaking, there are three
ways to do this:

1. Communicate the version out of band somehow. At Oxide we use this pattern
   for [Dropshot] API versions via an `api-version` header.
1. Store the version in an outer *envelope* object.
1. Try deserializing as the latest version, then attempt to deserialize as
   all prior versions walking back from the latest. We call this pattern
   *untagged deserialization*.

This crate implements the second option in a way that encodes best
practices, along with a way to migrate from the third option.

## Features

* A simple format with `version` and `versioned_data` fields. See *Format*
  below.
* Writing with a version attached. See [`WriteEnvelope`].
* Reading a fixed version, or upgrading from older versions on read. See
  [`read_json`] and *Usage* below.
* For pre-envelope data, falling back to
  [untagged deserialization][read_json_or_untagged]
  or to [a custom callback][read_json_or_else].
* Knowing whether to rewrite serialized data, with
  [`ReadOutput::needs_rewrite`].
* JSON Schema support with the `schemars08` feature.
* A focus on correctness and good error handling. (The envelope checks
  are careful and strict, and this crate does not use [`serde_json::Value`]
  when reading. See [the appendix](#appendix-why-not-serde_jsonvalue) for
  why.)
* No proc macros, minimal dependencies: only [`serde_core`] and
  [`serde_json`] by default.

## Format

The version is a `u32`, and is stored in a `version` field. The versioned
data itself is stored in a `versioned_data` field. For example:

````json
{
    "version": 1,
    "versioned_data": {
        "key": "value"
    }
}
````

For the rationale, see [the appendix](#appendix-format-rationale).

## Usage

### As fixed versions

Let’s say you have a CLI command with JSON output. Clients are expected to
know about the specific format version.

````rust
use oxide_versioned_envelope::{Versioned, WriteEnvelope, read_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ExecutionData {
    retries: u32,
}

// Define `Versioned` for the type.
impl Versioned for ExecutionData {
    const VERSION: u32 = 2;
}

// Let's say you have a value of this type.
let data = ExecutionData { retries: 3 };

// To serialize this value, use `WriteEnvelope`.
let bytes = serde_json::to_vec(&WriteEnvelope::new(&data))?;
assert_eq!(
    String::from_utf8(bytes.clone())?,
    r#"{"version":2,"versioned_data":{"retries":3}}"#
);

// To deserialize, use `read_json`. This verifies that
// the version is correct.
//
// On success, read_json (and all the other read methods) return
// a `ReadOutput`, which contains the requested data and information
// about where it comes from, whether it needs a rewrite, etc.
// To extract the inner value, call `into_value`.
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

// Implement `Versioned` for each version.
impl Versioned for SettingsV1 {
    const VERSION: u32 = 1;
}

impl Versioned for SettingsV2 {
    const VERSION: u32 = 2;
}

// Let's say you have this value that you serialize as
// V1.
let settings_v1 = SettingsV1 { string_setting: "hello".to_string() };

// To serialize this value, use `WriteEnvelope`.
let bytes = serde_json::to_vec(&WriteEnvelope::new(&settings_v1))?;
assert_eq!(
    String::from_utf8(bytes.clone())?,
    r#"{"version":1,"versioned_data":{"string_setting":"hello"}}"#
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
let upgraded: SettingsV2 = read.into_value();
assert_eq!(upgraded, expected);

// ---

// Sometimes you might have data on disk that was written before
// you adopted the envelope: here, a bare `SettingsV2` with no
// `version` field. In that case, `read_json_or_untagged` tries
// deserializing as an envelope, otherwise as each version in
// descending order. (See the `read_untagged_json` documentation
// for why this kind of duck typing is fragile.)
let legacy = serde_json::to_vec(&upgraded)?;
let from_legacy: SettingsV2 =
    read_json_or_untagged::<Settings>(&legacy)?.into_value();
assert_eq!(from_legacy, expected);
````

For more read helpers, including documentation for when to use them, see
[`read_json`].

## Cargo features

* `schemars08`: Enables JSON schema generation for `WriteEnvelope`.
  *Not enabled by default*.

## Minimum supported Rust version (MSRV)

This crate’s MSRV is **Rust 1.86**. In general we aim for 6 months of Rust
compatibility.

## Appendix: format rationale

This format is chosen to have the following properties:

* Avoid double-encoding the data as either a string or a `Vec<u8>`.
  Double-encoding is inefficient (especially `Vec<u8>`, which is usually
  serialized as a list of integers) and produces worse error messages.

* Make it likely that in serialized JSON, `version` comes before
  `versioned_data`. This crate’s [`WriteEnvelope`] always produces
  keys in that order, and because `version` is a prefix of `versioned_data`,
  the following scenarios also produce keys in the desired order:
  
  * Sorting keys in byte order, such as with `jq -S` or
    [`serde_json::Value`] without `preserve_order`.
  * Sorting keys by length and then by bytes, such as Postgres’s JSONB.
  
  Putting `version` before `versioned_data` means that a streaming reader,
  such as a hand-written serde [`Deserialize`] implementation, or a reader
  in another language, can read the version and then the data in one pass
  without buffering. This crate may add such a fast path in the future.
  
  Putting `version` before `versioned_data` is a property of the writer,
  not a requirement on the read side, since JSON objects are unordered.
  It can be an optimization on the read side, though.

* For [`read_json_or_untagged`], minimize confusion between untagged data
  and envelopes. `versioned_data` is a much less common field name than
  more obvious ones like `data`.

* Reject unknown fields, to catch damaged or misidentified documents. This
  does mean that adding an envelope-level field later would break existing
  readers; we accept this potential downside.

The format is similar to what serde calls [*adjacently
tagged*](https://serde.rs/enum-representations.html#adjacently-tagged). But
note that serde’s derived adjacently tagged enums require a string tag
(`"1"`, not `1`), so they can’t read this format directly.

We considered and rejected other tag formats for the following reasons:

* *Externally tagged*:
  
  ````json
  {
      "1": {
          "key": "value"
      }
  }
  ````
  
  This structurally guarantees that the version is seen before the data, but
  makes using `jq` on serialized data somewhat less convenient, and the
  version becomes a string that needs a canonical form.

* *Internally tagged*:
  
  ````json
  {
      "version": 1,
      "key": "value1",
      "key2": "value2"
  }
  ````
  
  This wouldn’t work if the data isn’t a JSON object, or if it contains a
  `version` key.

* *Untagged*: fragile; this crate provides a migration path from untagged
  inputs.

## Appendix: why not `serde_json::Value`?

For reading versioned data, a naive implementation might do something like:

````rust
struct ReadEnvelope {
    version: u32,
    versioned_data: serde_json::Value,
}

impl ReadEnvelope {
    fn deserialize_data<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.versioned_data.clone())
    }
}
````

But this has a few pitfalls:

1. [`serde_json::Value`] has funky behavior around objects in a few
   different ways.
   
   1. For a payload like `{"a":1,"a":2}`, [`serde_json::Value`] drops
      the first `a` and only keeps the last one. This crate lets the
      deserialized type decide what to do with duplicate keys (a derived
      [`Deserialize`] would typically reject duplicate keys).
      
      There is no workaround for this with `serde_json::Value`.
   
   1. Object key order is only preserved if the `preserve_order` feature
      is enabled. This can lead to surprising roundtrip behavior. There are
      a couple different options, neither of which is satisfying:
      
      * Enable the `preserve_order` feature, which affects
        all other crates in the Cargo dependency graph.
      * Accept that key order might not roundtrip.
      
      This feature-flag coupling is hard to explain to users; we can do much
      better than this.

1. With default features, [`serde_json::Value`] cannot represent certain
   types that serde itself supports deserialization for, such as `u128`s.
   Like with `preserve_order` above, turning on `arbitrary_precision` would
   fix this, but that is undesirable for the same reasons.

1. If deserialization fails, the error message doesn’t carry
   a line/column offset.

An in-between option is [`serde_json::value::RawValue`], which allows part
of a JSON payload to be stored as a string, and addresses points 1 and 2.
But for point 3, `RawValue` produces incorrect line and column offsets on
error (the offsets produced are relative to the captured part of the
payload, not relative to the full payload). Incorrect values are worse than
not having the line and column offsets at all.

Instead of either option, we perform passes over the underlying JSON
directly:

1. The first pass validates that the JSON is well-formed, and for
   envelope reads, records which top-level fields are present.
1. For envelope reads, the second pass reads the version, ignoring the
   data.
1. Then, depending on the read method called, we perform one or
   more deserialization passes through the [`VersionSet::parse`]
   callback.

Why not read everything in one streaming pass?

* Despite our best efforts, `versioned_data` may still come before
  `version` (see the format rationale). Handling that in one pass
  means internally buffering the data until the version is known —
  something this crate takes care to avoid, since internal buffering
  has many of the same problems as [`serde_json::Value`].

* For data that might not be an envelope, `version` can’t be interpreted
  until every key has been seen. For example,
  `{"version": "1.2.3", "name": "x"}` is not a versioned envelope and
  should only be interpreted as untagged.

The cost is that reading takes roughly twice as long as a plain
[`serde_json::from_slice`], and the read functions take `&[u8]` rather than
an arbitrary reader. In the future, this crate may add a fast path for the
common case where the fields are in order.

[Dropshot]: https://docs.rs/dropshot
[`WriteEnvelope`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/write/imp/struct.WriteEnvelope.html "struct oxide_versioned_envelope::write::imp::WriteEnvelope"
[`read_json`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json.html "fn oxide_versioned_envelope::read::imp::read_json"
[read_json_or_untagged]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json_or_untagged.html "fn oxide_versioned_envelope::read::imp::read_json_or_untagged"
[read_json_or_else]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json_or_else.html "fn oxide_versioned_envelope::read::imp::read_json_or_else"
[`ReadOutput::needs_rewrite`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/output/struct.ReadOutput.html#method.needs_rewrite "method oxide_versioned_envelope::read::output::ReadOutput::needs_rewrite"
[`serde_json::Value`]: https://docs.rs/serde_json/1.0.151/serde_json/value/enum.Value.html "enum serde_json::value::Value"
[`serde_core`]: https://docs.rs/serde_core/1.0.229/serde_core/index.html "mod serde_core"
[`serde_json`]: https://docs.rs/serde_json/1.0.151/serde_json/index.html "mod serde_json"
[`Deserialize`]: https://docs.rs/serde_core/1.0.229/serde_core/de/trait.Deserialize.html "trait serde_core::de::Deserialize"
[`read_json_or_untagged`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/read/imp/fn.read_json_or_untagged.html "fn oxide_versioned_envelope::read::imp::read_json_or_untagged"
[`serde_json::value::RawValue`]: https://docs.rs/serde_json/1.0.151/serde_json/raw/struct.RawValue.html "struct serde_json::raw::RawValue"
[`VersionSet::parse`]: https://docs.rs/oxide-versioned-envelope/0.1.0/oxide_versioned_envelope/versioned/trait.VersionSet.html#tymethod.parse "associated function oxide_versioned_envelope::versioned::VersionSet::parse"
[`serde_json::from_slice`]: https://docs.rs/serde_json/1.0.151/serde_json/de/fn.from_slice.html "fn serde_json::de::from_slice"
<!-- cargo-sync-rdme ]] -->

## License

This project is available under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT license](LICENSE-MIT).
