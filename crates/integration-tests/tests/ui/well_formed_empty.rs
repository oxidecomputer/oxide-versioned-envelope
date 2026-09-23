use oxide_versioned_envelope::{
    Step, VersionSet, assert_version_set_well_formed,
};
use serde::Deserializer;

struct Empty;

impl VersionSet for Empty {
    type Latest = ();
    type Context = ();
    const VERSIONS: &'static [u32] = &[];

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

const _: () = assert_version_set_well_formed::<Empty>();

fn main() {}
