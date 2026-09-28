use oxide_versioned_envelope::{Versioned, version_set};

#[derive(serde::Deserialize)]
struct V1Payload;

#[derive(serde::Deserialize)]
struct V2Payload;

#[derive(serde::Deserialize)]
struct V3Payload;

impl Versioned for V1Payload {
    const VERSION: u32 = 1;
}

impl Versioned for V2Payload {
    const VERSION: u32 = 2;
}

impl Versioned for V3Payload {
    const VERSION: u32 = 3;
}

impl From<V1Payload> for V2Payload {
    fn from(_: V1Payload) -> Self {
        Self
    }
}

impl From<V2Payload> for V3Payload {
    fn from(_: V2Payload) -> Self {
        Self
    }
}

version_set! {
    enum OlderDoesNotStep -> V3Payload {
        V1(V1Payload) => V2,
        V2(V2Payload),
        V3(V3Payload),
    }
}

fn main() {}
