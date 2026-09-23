//! Fixtures for oxide-versioned-envelope integration tests.

use hegel::DefaultGenerator;
use oxide_versioned_envelope::{Versioned, WriteEnvelope, version_set};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(
    Clone, Debug, Eq, PartialEq, Serialize, Deserialize, DefaultGenerator,
)]
pub struct SettingsV1 {
    pub value: u32,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Serialize, Deserialize, DefaultGenerator,
)]
pub struct SettingsV2 {
    pub value: u32,
    pub label: Option<String>,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Serialize, Deserialize, DefaultGenerator,
)]
#[cfg_attr(
    feature = "schemars08",
    derive(schemars08::JsonSchema),
    schemars(crate = "schemars08")
)]
pub struct SettingsV4 {
    pub value: u32,
    pub label: String,
}

impl Versioned for SettingsV1 {
    const VERSION: u32 = 1;
}

impl Versioned for SettingsV2 {
    const VERSION: u32 = 2;
}

impl Versioned for SettingsV4 {
    const VERSION: u32 = 4;
}

impl From<SettingsV1> for SettingsV2 {
    fn from(v1: SettingsV1) -> Self {
        Self { value: v1.value, label: Some("upgraded from v1".to_owned()) }
    }
}

#[derive(Debug, Error)]
#[error("missing a label as part of v2 -> v4 upgrade")]
pub struct MissingLabel;

impl TryFrom<SettingsV2> for SettingsV4 {
    type Error = MissingLabel;

    fn try_from(v2: SettingsV2) -> Result<Self, MissingLabel> {
        let label = v2.label.ok_or(MissingLabel)?;
        Ok(Self { value: v2.value, label })
    }
}

version_set! {
    #[derive(Debug)]
    pub enum Settings -> SettingsV4 {
        V1(SettingsV1) => V2,
        V2(SettingsV2) => try V4,
        V4(SettingsV4),
    }
}

pub const SETTINGS_VERSIONS: [u32; 3] = [1, 2, 4];

#[derive(Clone, Debug, DefaultGenerator)]
pub enum AnyPayload {
    V1(SettingsV1),
    V2(SettingsV2),
    V4(SettingsV4),
}

impl AnyPayload {
    pub fn version(&self) -> u32 {
        match self {
            Self::V1(_) => SettingsV1::VERSION,
            Self::V2(_) => SettingsV2::VERSION,
            Self::V4(_) => SettingsV4::VERSION,
        }
    }

    pub fn write(&self) -> Vec<u8> {
        match self {
            Self::V1(payload) => {
                serde_json::to_vec(&WriteEnvelope::new(payload))
            }
            Self::V2(payload) => {
                serde_json::to_vec(&WriteEnvelope::new(payload))
            }
            Self::V4(payload) => {
                serde_json::to_vec(&WriteEnvelope::new(payload))
            }
        }
        .expect("the envelope serialized to JSON")
    }

    pub fn upgrade(self) -> Result<SettingsV4, MissingLabel> {
        match self {
            Self::V1(payload) => SettingsV2::from(payload).try_into(),
            Self::V2(payload) => payload.try_into(),
            Self::V4(payload) => Ok(payload),
        }
    }
}
