use integration_tests::SettingsV4;
use oxide_versioned_envelope::WriteEnvelope;
use schemars08::JsonSchema;

#[test]
fn matches_the_snapshot() {
    let schema = schemars08::schema_for!(WriteEnvelope<SettingsV4>);
    let mut text = serde_json::to_string_pretty(&schema)
        .expect("the schema serialized to JSON");
    text.push('\n');
    expectorate::assert_contents("tests/output/settings-envelope.json", &text);
}

#[test]
fn schema_id_includes_payload() {
    mod other {
        #[derive(schemars08::JsonSchema)]
        #[schemars(crate = "schemars08")]
        pub struct SettingsV4;

        impl oxide_versioned_envelope::Versioned for SettingsV4 {
            const VERSION: u32 = 4;
        }
    }

    assert_ne!(
        <WriteEnvelope<SettingsV4> as JsonSchema>::schema_id(),
        <WriteEnvelope<other::SettingsV4> as JsonSchema>::schema_id(),
    );
}
