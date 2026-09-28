//! Tests for valid and invalid JSON.

use hegel::{TestCase, generators as gs};
use integration_tests::{AnyPayload, Settings};
use oxide_versioned_envelope::{
    errors::{EnvelopeError, ReadError},
    read_json,
};
use serde_json::Value;

/// Asserts that the reader and `serde_json::Value` match exactly on which JSON
/// is valid.
#[hegel::test]
fn reader_agrees_with_serde_json(tc: TestCase) {
    let mut bytes = tc.draw(gs::default::<AnyPayload>()).write();
    let at = tc.draw(gs::integers::<usize>().max_value(bytes.len()));

    // These transformations almost always make the JSON invalid. See
    // `envelopes_are_read` for tests against valid JSON.
    if tc.draw(gs::booleans()) {
        bytes.truncate(at);
    } else {
        let inserted =
            tc.draw(gs::vecs(gs::integers::<u8>()).min_size(1).max_size(3));
        bytes.splice(at..at, inserted);
    }

    let read = read_json::<Settings>(&bytes);
    match serde_json::from_slice::<Value>(&bytes) {
        Err(expected) => {
            let Err(ReadError::Envelope(EnvelopeError::Json { source })) = read
            else {
                panic!(
                    "serde_json refuses the document, but the reader returned {read:?}"
                );
            };
            assert_eq!(
                (source.line(), source.column()),
                (expected.line(), expected.column())
            );
        }
        Ok(_) => {
            if let Err(ReadError::Envelope(EnvelopeError::Json { source })) =
                read
            {
                panic!(
                    "serde_json accepts the document, but the reader refused it: {source}"
                );
            }
        }
    }
}
