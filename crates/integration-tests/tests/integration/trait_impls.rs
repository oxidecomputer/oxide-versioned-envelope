//! Static assertions for trait impls.

use integration_tests::SettingsV4;
use oxide_versioned_envelope::{
    WriteEnvelope,
    errors::{ReadError, ReadOrUntaggedError, UntaggedError},
};
use serde::de::DeserializeOwned;
use static_assertions::{assert_impl_all, assert_not_impl_any};
use std::error::Error;

// Ensure that this crate's errors can be converted to anyhow errors, etc.
assert_impl_all!(ReadError: Error, Send, Sync);
assert_impl_all!(UntaggedError: Error, Send, Sync);
assert_impl_all!(ReadOrUntaggedError: Error, Send, Sync);

// WriteEnvelope should *not* implement Deserialize -- callers must go through
// the `read_*` methods.
assert_not_impl_any!(WriteEnvelope<SettingsV4>: DeserializeOwned);
