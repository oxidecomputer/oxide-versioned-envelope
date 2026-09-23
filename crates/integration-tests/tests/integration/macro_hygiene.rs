//! Tests for macro hygiene.

use integration_tests::{AnyPayload, Settings};
use oxide_versioned_envelope::read_json;

// Ensure that all paths emitted by the `version_set` macro are fully qualified.
mod no_prelude {
    #![no_implicit_prelude]

    use ::integration_tests::{SettingsV1, SettingsV2, SettingsV4};

    // The macro shouldn't pick this up.
    #[expect(dead_code)]
    const V4: u32 = 0;

    ::oxide_versioned_envelope::version_set! {
        pub(crate) enum Settings -> SettingsV4 {
            V1(SettingsV1) => V2,
            #[doc(hidden)]
            V2(SettingsV2) => try V4,
            V4(SettingsV4)
        }
    }
}

#[hegel::test]
fn macro_expansion_under_no_prelude(tc: hegel::TestCase) {
    let bytes = tc.draw(hegel::generators::default::<AnyPayload>()).write();
    let expected = format!("{:?}", read_json::<Settings>(&bytes));
    assert_eq!(
        format!("{:?}", read_json::<no_prelude::Settings>(&bytes)),
        expected
    );
}
