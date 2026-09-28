//! Tests for error messages produced by oxide-versioned-envelope.

use integration_tests::{Settings, Thresholds};
use oxide_versioned_envelope::{
    read_json, read_json_or_else, read_json_or_untagged, read_untagged_json,
};
use std::error::Error;
use swrite::{SWrite, swriteln};
use thiserror::Error;

const SNAPSHOT_FILE: &str = "tests/output/messages.txt";

#[derive(Debug, Error)]
#[error("the legacy reader found no settings in the document")]
struct LegacyRefused;

fn chain(error: &(dyn Error + 'static)) -> Vec<String> {
    std::iter::successors(Some(error), |&error| error.source())
        .map(ToString::to_string)
        .collect()
}

fn refusal<T, E: Error + 'static>(result: Result<T, E>) -> Vec<String> {
    match result {
        Ok(_) => panic!("the document is refused"),
        Err(error) => chain(&error),
    }
}

#[test]
fn message_snapshots() {
    let too_deep = format!(
        r#"{{"version":4,"versioned_data":{{}},"x":{}{}}}"#,
        "[".repeat(200),
        "]".repeat(200)
    );
    let many_unknown = format!(
        r#"{{"version":4,"versioned_data":{{}},{}}}"#,
        (0..12)
            .map(|index| format!(r#""extra{index:02}":{index}"#))
            .collect::<Vec<_>>()
            .join(",")
    );
    let envelope_cases: Vec<(&str, &[u8])> = vec![
        ("not JSON", b"not json at all"),
        ("truncated", br#"{"version":4,"versioned_data":{"value":1}"#),
        (
            "a lone surrogate",
            br#"{"version":4,"versioned_data":{},"x":"\ud800"}"#,
        ),
        (
            "invalid UTF-8",
            b"{\"version\":4,\"versioned_data\":{},\"x\":\"\xff\"}",
        ),
        ("nesting past the recursion limit", too_deep.as_bytes()),
        ("a null root", b"null"),
        ("an array root", br#"[{"version":4,"versioned_data":{}}]"#),
        ("a float root", b"1.5"),
        ("an integer root past u64::MAX", b"18446744073709551616"),
        ("no version", br#"{"versioned_data":{"value":1}}"#),
        ("no data", br#"{"version":4}"#),
        ("neither field", br#"{"meta":{"version":4,"versioned_data":{}}}"#),
        (
            "a repeated version",
            br#"{"version":4,"version":2,"versioned_data":{}}"#,
        ),
        (
            "repeated data",
            br#"{"version":4,"versioned_data":{},"versioned_data":{}}"#,
        ),
        (
            "both repeated",
            br#"{"version":4,"version":2,"versioned_data":{},"versioned_data":{}}"#,
        ),
        (
            "unknown fields",
            br#"{"version":4,"meta":{},"versioned_data":{},"Version":4}"#,
        ),
        (
            "a repeated unknown field",
            br#"{"version":4,"x":1,"versioned_data":{},"meta":{},"x":2}"#,
        ),
        (
            "unknown fields with special characters",
            br#"{"version":4,"versioned_data":{},"a`b":1,"line\nbreak":2,"\u001b[31m":3}"#,
        ),
        ("more unknown fields than are shown", many_unknown.as_bytes()),
        (
            "a version that is a string",
            br#"{"version":"4","versioned_data":{}}"#,
        ),
        ("a version of -0", br#"{"version":-0,"versioned_data":{}}"#),
        (
            "a version past u32::MAX",
            br#"{"version":4294967296,"versioned_data":{}}"#,
        ),
        (
            "a version newer than the set",
            br#"{"version":5,"versioned_data":{}}"#,
        ),
        (
            "a version older than the set",
            br#"{"version":0,"versioned_data":{}}"#,
        ),
        (
            "a version in a gap of the set",
            br#"{"version":3,"versioned_data":{}}"#,
        ),
        (
            "data that is not the version's payload",
            br#"{"version":2,"versioned_data":{"value":"seven"}}"#,
        ),
        (
            "a conversion that fails",
            br#"{"version":2,"versioned_data":{"value":1,"label":null}}"#,
        ),
    ];

    let mut sections: Vec<(&str, &str, &[u8], Vec<String>)> = Vec::new();
    for (name, bytes) in envelope_cases {
        let messages = refusal(read_json::<Settings>(bytes));
        sections.push(("read_json", name, bytes, messages));
    }
    let bytes = &br#"{"version":1,"versioned_data":{"warn":60}}"#[..];
    let messages = refusal(read_json::<Thresholds>(bytes));
    sections.push((
        "read_json",
        "a conversion that fails after an earlier step",
        bytes,
        messages,
    ));
    for (name, bytes) in [
        ("no version's payload", &b"7"[..]),
        ("the same error from every version", br#"{"value":"x"}"#),
        ("a conversion that fails", br#"{"value":7}"#),
    ] {
        let messages = refusal(read_untagged_json::<Settings>(bytes));
        sections.push(("read_untagged_json", name, bytes, messages));
    }
    let bytes = &b"7"[..];
    let messages = refusal(read_json_or_untagged::<Settings>(bytes));
    sections.push(("read_json_or_untagged", "neither", bytes, messages));
    let bytes = &br#"{"value":1}"#[..];
    let messages = refusal(read_json_or_else::<Settings, _>(bytes, |_| {
        Err(LegacyRefused.into())
    }));
    sections.push((
        "read_json_or_else",
        "a refusing fallback",
        bytes,
        messages,
    ));

    let mut snapshots = String::new();
    for (reader, name, bytes, messages) in sections {
        for pair in messages.windows(2) {
            assert_ne!(pair[0], pair[1], "the chain repeats a message");
        }
        swriteln!(snapshots, "## {reader}: {name}");
        swriteln!(snapshots, "input: {}", String::from_utf8_lossy(bytes));
        swriteln!(snapshots, "error: {}", messages[0]);
        for cause in &messages[1..] {
            swriteln!(snapshots, "  caused by: {cause}");
        }
        snapshots.push('\n');
    }
    expectorate::assert_contents(SNAPSHOT_FILE, &snapshots);
}
