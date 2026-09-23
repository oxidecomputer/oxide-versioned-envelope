use oxide_versioned_envelope::{Versioned, version_set};

#[derive(serde::Deserialize)]
struct V1Payload;

#[derive(serde::Deserialize)]
struct V2Payload;

impl Versioned for V1Payload {
    const VERSION: u32 = 1;
}

impl Versioned for V2Payload {
    const VERSION: u32 = 2;
}

impl From<V1Payload> for V2Payload {
    fn from(_: V1Payload) -> Self {
        Self
    }
}

version_set! {
    enum NewestSteps -> V2Payload {
        V1(V1Payload) => V2,
        V2(V2Payload) => V2,
    }
}

fn main() {}
