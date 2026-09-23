use oxide_versioned_envelope::{
    Step, VersionSet, assert_version_set_well_formed,
};
use serde::Deserializer;

struct Descending;

impl VersionSet for Descending {
    type Latest = ();
    type Context = ();
    const VERSIONS: &'static [u32] = &[2, 1];

    fn parse<'de, D: Deserializer<'de>>(
        _version: u32,
        _deserializer: D,
    ) -> Result<Self, D::Error> {
        Ok(Self)
    }

    fn version(&self) -> u32 {
        1
    }

    fn step(self, _cx: &()) -> Step<Self> {
        Step::Done(())
    }
}

const _: () = assert_version_set_well_formed::<Descending>();

fn main() {}
