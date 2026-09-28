//! Tests for panics when a hand-written `VersionSet` breaks the trait's requirements.

use oxide_versioned_envelope::{Step, VersionSet, read_json};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
struct Scripted {
    version: u32,
    // The next steps are dictated by the value of `ScriptedStep` -- this is not
    // code anyone would normally write, but allows us to compactly represent a
    // variety of tests by altering the input payload.
    steps: Vec<ScriptedStep>,
}

#[derive(Deserialize)]
enum ScriptedStep {
    Next(u32),
    Done,
    Failed(u32),
}

impl VersionSet for Scripted {
    type Latest = ();
    type Context = ();
    const VERSIONS: &'static [u32] = &[1, 2, 3];

    fn parse<'de, D: Deserializer<'de>>(
        _version: u32,
        deserializer: D,
    ) -> Result<Self, D::Error> {
        Self::deserialize(deserializer)
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn step(mut self, _cx: &()) -> Step<Self> {
        match self.steps.remove(0) {
            ScriptedStep::Next(version) => Step::Next(Self { version, ..self }),
            ScriptedStep::Done => Step::Done(()),
            ScriptedStep::Failed(to) => {
                Step::Failed { to, source: "the script failed".into() }
            }
        }
    }
}

fn read_scripted(version: u32, data: &str) {
    let bytes = format!(r#"{{"version":{version},"versioned_data":{data}}}"#);
    let _ = read_json::<Scripted>(bytes.as_bytes());
}

#[test]
#[should_panic(
    expected = "parse was asked for version 1, but returned a version 2 value"
)]
fn parse_returns_a_different_version() {
    read_scripted(1, r#"{"version":2,"steps":[{"Next":3},"Done"]}"#);
}

#[test]
#[should_panic(
    expected = "step for version 2 returned Next naming version 1, which is not newer"
)]
fn step_goes_backwards() {
    read_scripted(2, r#"{"version":2,"steps":[{"Next":1}]}"#);
}

#[test]
#[should_panic(
    expected = "step for version 1 returned Failed naming version 4, but 4 is not in VERSIONS"
)]
fn step_fails_to_an_unlisted_version() {
    read_scripted(1, r#"{"version":1,"steps":[{"Failed":4}]}"#);
}

#[test]
#[should_panic(
    expected = "step for version 1 returned Done, but the last version in VERSIONS is 3"
)]
fn step_finishes_early() {
    read_scripted(1, r#"{"version":1,"steps":["Done"]}"#);
}

#[test]
#[should_panic(
    expected = "step for version 3, the last version in VERSIONS, returned Next naming version 3"
)]
fn latest_version_steps() {
    read_scripted(3, r#"{"version":3,"steps":[{"Next":3}]}"#);
}
