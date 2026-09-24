use super::WriteEnvelope;
use crate::{
    versioned::Versioned,
    wire::{DATA_FIELD, VERSION_FIELD},
};
use schemars08::{
    JsonSchema, Map, Set,
    r#gen::SchemaGenerator,
    schema::{InstanceType, Metadata, ObjectValidation, Schema, SchemaObject},
};
use std::borrow::Cow;

const ENVELOPE_DESCRIPTION: &str =
    "A versioned envelope, where `data` is interpreted according to `version`.";

#[cfg_attr(doc_cfg, doc(cfg(feature = "schemars08")))]
impl<T: Versioned + JsonSchema> JsonSchema for WriteEnvelope<T> {
    fn schema_name() -> String {
        format!("{}Envelope", T::schema_name())
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Owned(format!(
            "oxide_versioned_envelope::WriteEnvelope<{}>",
            T::schema_id()
        ))
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        // The version is specified to be exactly this value.
        let mut version =
            <u32 as JsonSchema>::json_schema(generator).into_object();
        version.enum_values = Some(vec![serde_json::Value::from(T::VERSION)]);

        let data = generator.subschema_for::<T>();

        let mut properties = Map::new();
        properties.insert(VERSION_FIELD.to_owned(), version.into());
        properties.insert(DATA_FIELD.to_owned(), data);

        let mut required = Set::new();
        required.insert(VERSION_FIELD.to_owned());
        required.insert(DATA_FIELD.to_owned());

        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(ENVELOPE_DESCRIPTION.to_owned()),
                ..Metadata::default()
            })),
            instance_type: Some(InstanceType::Object.into()),
            object: Some(Box::new(ObjectValidation {
                properties,
                required,
                // The read_* functions reject additional fields, so match that
                // in the schema.
                additional_properties: Some(Box::new(Schema::Bool(false))),
                ..ObjectValidation::default()
            })),
            ..SchemaObject::default()
        }
        .into()
    }
}
