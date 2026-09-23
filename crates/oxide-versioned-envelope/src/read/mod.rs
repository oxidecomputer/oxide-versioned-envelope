mod document;
mod imp;
mod output;
mod validate;

pub use imp::{
    read_json, read_json_or_else, read_json_or_else_with,
    read_json_or_untagged, read_json_or_untagged_with, read_json_with,
    read_untagged_json, read_untagged_json_with,
};
pub use output::{Origin, ReadOutput};
