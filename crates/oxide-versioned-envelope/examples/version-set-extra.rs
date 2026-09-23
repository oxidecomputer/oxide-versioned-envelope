//! An example showing advanced features in the `version_set` macro.
//!
//! This example demonstrates:
//!
//! * Conversion to an `Arc` type.
//! * Jumping from an older version directly to a newer one.

use oxide_versioned_envelope::{
    Versioned, WriteEnvelope, read_json, version_set,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
struct SettingsV1 {
    verbose: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SettingsV2 {
    level: u8,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SettingsV3 {
    level: u8,
    color: bool,
}

impl Versioned for SettingsV1 {
    const VERSION: u32 = 1;
}

impl Versioned for SettingsV2 {
    const VERSION: u32 = 2;
}

impl Versioned for SettingsV3 {
    const VERSION: u32 = 3;
}

// In this example, it is possible to convert from V1 straight to V3, without
// going through V2.
impl From<SettingsV1> for SettingsV3 {
    fn from(v1: SettingsV1) -> Self {
        Self { level: if v1.verbose { 2 } else { 1 }, color: false }
    }
}

impl From<SettingsV2> for SettingsV3 {
    fn from(v2: SettingsV2) -> Self {
        Self { level: v2.level, color: false }
    }
}

// In this example, `Latest` is `Arc<SettingsV3>` rather than `SettingsV3`.
// This works because the last step does an `Into::into` at the end.
version_set! {
    enum Settings -> Arc<SettingsV3> {
        V1(SettingsV1) => V3,
        V2(SettingsV2) => V3,
        V3(SettingsV3),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let documents = [
        (
            "version 1",
            serde_json::to_vec(&WriteEnvelope::new(&SettingsV1 {
                verbose: true,
            }))?,
        ),
        (
            "version 2",
            serde_json::to_vec(&WriteEnvelope::new(&SettingsV2 { level: 3 }))?,
        ),
        (
            "version 3",
            serde_json::to_vec(&WriteEnvelope::new(&SettingsV3 {
                level: 4,
                color: true,
            }))?,
        ),
    ];

    for (label, document) in documents {
        let settings: Arc<SettingsV3> =
            read_json::<Settings>(&document)?.into_value();
        println!("{label}: {settings:?}");
    }

    Ok(())
}
