use serde_core::de::{DeserializeOwned, Deserializer};

/// Represents a single versioned type for writing and reading data.
///
/// * To write a versioned envelope, use
///   [`WriteEnvelope`](crate::WriteEnvelope) with a type that implements
///   this trait.
/// * To read a versioned envelope at a _fixed_ version,
///   use [`read_json`](crate::read_json) with a [`Versioned`] type.
/// * To read a versioned envelope in an _upgradable_ fashion,
///   implement [`Versioned`] and [`VersionSet`], the latter using the
///   [`version_set!`](crate::version_set!) macro if possible.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{Versioned, WriteEnvelope, read_json};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
/// struct Snapshot {
///     generation: u64,
/// }
///
/// impl Versioned for Snapshot {
///     const VERSION: u32 = 3;
/// }
///
/// let snapshot = Snapshot { generation: 7 };
/// let bytes = serde_json::to_vec(&WriteEnvelope::new(&snapshot))?;
/// assert_eq!(
///     String::from_utf8(bytes.clone())?,
///     r#"{"version":3,"data":{"generation":7}}"#
/// );
/// assert_eq!(read_json::<Snapshot>(&bytes)?.into_value(), snapshot);
/// # Ok(()) }
/// ```
pub trait Versioned {
    /// The version of this type.
    const VERSION: u32;
}

// This impl lets a caller write `WriteEnvelope::new(&payload)`.
impl<T: Versioned + ?Sized> Versioned for &T {
    const VERSION: u32 = T::VERSION;
}

/// Represents a set of versioned types for deserializing upgradable data.
///
/// In most cases, use the [`version_set!`](crate::version_set) macro to
/// implement this trait. But the trait can also be implemented manually in case
/// you need to express upgrade mechanisms that aren't supported by the macro
/// (e.g., when the upgrade process needs ancillary data).
///
/// # Examples
///
/// Here's an example of a manually-implemented `VersionSet` when ancillary data
/// is required:
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Step, VersionSet, Versioned, WriteEnvelope,
///     assert_version_set_well_formed, read_json_with,
/// };
/// use serde::{Deserialize, Deserializer, Serialize};
///
/// // Version 1 of this data only had a `retries` field.
/// #[derive(Debug, Deserialize, Serialize)]
/// struct LimitsV1 {
///     retries: u32,
/// }
///
/// // Version 2 added a `max_connections` field.
/// #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
/// struct LimitsV2 {
///     retries: u32,
///     max_connections: u32,
/// }
///
/// impl Versioned for LimitsV1 {
///     const VERSION: u32 = 1;
/// }
///
/// impl Versioned for LimitsV2 {
///     const VERSION: u32 = 2;
/// }
///
/// // This data is required to upgrade v1 data to v2.
/// struct HostDefaults {
///     max_connections: u32,
/// }
///
/// impl LimitsV2 {
///     // This implements the conversion from v1.
///     fn from_v1(v1: LimitsV1, defaults: &HostDefaults) -> Self {
///         Self {
///             retries: v1.retries,
///             max_connections: defaults.max_connections,
///         }
///     }
/// }
///
/// // To use `VersionSet`, first define an enum with all the versions.
/// enum Limits {
///     V1(LimitsV1),
///     V2(LimitsV2),
/// }
///
/// // Then implement the trait.
/// impl VersionSet for Limits {
///     // The latest version.
///     type Latest = LimitsV2;
///
///     // The context (ancillary data) required to upgrade versions.
///     type Context = HostDefaults;
///
///     // The list of versions: must be strictly increasing.
///     const VERSIONS: &'static [u32] =
///         &[LimitsV1::VERSION, LimitsV2::VERSION];
///
///     // This method parses the data for a given version.
///     fn parse<'de, D: Deserializer<'de>>(
///         version: u32,
///         deserializer: D,
///     ) -> Result<Self, D::Error> {
///         match version {
///             LimitsV1::VERSION => {
///                 LimitsV1::deserialize(deserializer).map(Self::V1)
///             }
///             LimitsV2::VERSION => {
///                 LimitsV2::deserialize(deserializer).map(Self::V2)
///             }
///             _ => unreachable!("version {version} is not in `VERSIONS`"),
///         }
///     }
///
///     // This method returns the version number of a given instance.
///     fn version(&self) -> u32 {
///         match self {
///             Self::V1(_) => LimitsV1::VERSION,
///             Self::V2(_) => LimitsV2::VERSION,
///         }
///     }
///
///     // This method performs a single upgrade step.
///     fn step(self, defaults: &HostDefaults) -> Step<Self> {
///         match self {
///             Self::V1(limits) => {
///                 Step::Next(Self::V2(LimitsV2::from_v1(limits, defaults)))
///             }
///             Self::V2(limits) => Step::Done(limits),
///         }
///     }
/// }
///
/// // Be sure to call this function in this manner. This ensures
/// // at compile time that the version set is well-formed.
/// const _: () = assert_version_set_well_formed::<Limits>();
///
/// let defaults = HostDefaults { max_connections: 128 };
///
/// // Let's say you had some data in the v1 format.
/// let v1 = serde_json::to_vec(&WriteEnvelope::new(&LimitsV1 { retries: 3 }))?;
/// // Use `read_json_with` to convert to the latest version.
/// assert_eq!(
///     read_json_with::<Limits>(&v1, &defaults)?.into_value(),
///     LimitsV2 { retries: 3, max_connections: 128 },
/// );
///
/// // Data already in the latest version does not need to be converted.
/// let v2 = serde_json::to_vec(&WriteEnvelope::new(&LimitsV2 {
///     retries: 3,
///     max_connections: 64,
/// }))?;
/// assert_eq!(
///     read_json_with::<Limits>(&v2, &defaults)?.into_value(),
///     LimitsV2 { retries: 3, max_connections: 64 },
/// );
/// # Ok(()) }
/// ```
pub trait VersionSet: Sized {
    /// The type of the latest version.
    type Latest;

    /// Ancillary data required for conversion.
    ///
    /// In cases where no additional data is required, this is `()`. The
    /// [`version_set!`](crate::version_set) macro always sets this type to
    /// `()`.
    type Context: ?Sized;

    /// The list of versions.
    ///
    /// This must be strictly ascending and non-empty, with the newest version
    /// last. (This is validated by [`assert_version_set_well_formed`].)
    const VERSIONS: &'static [u32];

    /// Deserializes a value of the specified version.
    ///
    /// The deserializer is positioned at:
    ///
    /// * For versioned envelopes, immediately inside the envelope's `data` field.
    /// * For untagged payloads, the whole document.
    ///
    /// The deserializer must consume exactly one value.
    ///
    /// This crate guarantees that `version` is always one of [`Self::VERSIONS`].
    fn parse<'de, D: Deserializer<'de>>(
        version: u32,
        deserializer: D,
    ) -> Result<Self, D::Error>;

    /// Returns the version of this value.
    ///
    /// The implementer must ensure that the version is always one of
    /// [`Self::VERSIONS`]. Otherwise, the crate will panic at runtime.
    fn version(&self) -> u32;

    /// Returns the next step in the conversion process.
    ///
    /// A legal conversion step is one that returns a version greater than the
    /// input. Typically, this is the next version, but you're allowed to skip
    /// over versions if it makes sense that way. But conversion to an older
    /// version will panic at runtime.
    ///
    /// This is used by the library to drive the conversion loop.
    fn step(self, cx: &Self::Context) -> Step<Self>;
}

/// A single step in a read-time conversion process.
///
/// Returned by [`VersionSet::step`].
pub enum Step<S: VersionSet> {
    /// Conversion has advanced to a newer version.
    Next(S),

    /// Conversion to the latest version is completed.
    Done(S::Latest),

    /// Conversion failed.
    Failed {
        /// The version to which conversion failed.
        to: u32,

        /// The error that occurred.
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

/// Performs compile-time checks to ensure a [`VersionSet`] is well-formed.
///
/// Currently, this validates that:
///
/// * There's at least one version in [`VersionSet::VERSIONS`].
/// * The versions in that slice are in strictly increasing order.
///
/// # Examples
///
/// This is allowed:
///
/// ```
/// use oxide_versioned_envelope::{
///     Step, VersionSet, assert_version_set_well_formed,
/// };
///
/// struct Ascending;
///
/// impl VersionSet for Ascending {
///     type Latest = ();
///     type Context = ();
///     const VERSIONS: &'static [u32] = &[1, 2];
/// #   fn parse<'de, D: serde::Deserializer<'de>>(
/// #       _version: u32,
/// #       _deserializer: D,
/// #   ) -> Result<Self, D::Error> {
/// #       Ok(Self)
/// #   }
/// #   fn version(&self) -> u32 {
/// #       2
/// #   }
/// #   fn step(self, _cx: &()) -> Step<Self> {
/// #       Step::Done(())
/// #   }
/// }
///
/// const _: () = assert_version_set_well_formed::<Ascending>();
/// ```
///
/// This is not:
///
/// ```compile_fail,E0080
/// use oxide_versioned_envelope::{
///     Step, VersionSet, assert_version_set_well_formed,
/// };
///
/// struct Descending;
///
/// impl VersionSet for Descending {
///     type Latest = ();
///     type Context = ();
///     const VERSIONS: &'static [u32] = &[2, 1];
/// #   fn parse<'de, D: serde::Deserializer<'de>>(
/// #       _version: u32,
/// #       _deserializer: D,
/// #   ) -> Result<Self, D::Error> {
/// #       Ok(Self)
/// #   }
/// #   fn version(&self) -> u32 {
/// #       1
/// #   }
/// #   fn step(self, _cx: &()) -> Step<Self> {
/// #       Step::Done(())
/// #   }
/// }
///
/// const _: () = assert_version_set_well_formed::<Descending>();
/// ```
pub const fn assert_version_set_well_formed<S: VersionSet>() {
    let versions = S::VERSIONS;

    assert!(
        !versions.is_empty(),
        "a version set must list at least one version in `VersionSet::VERSIONS`"
    );

    let mut index = 1;
    while index < versions.len() {
        assert!(
            versions[index - 1] < versions[index],
            "a version set's `VersionSet::VERSIONS` must be strictly ascending, \
             oldest version first"
        );
        index += 1;
    }
}

/// A blanket implementation for non-upgradable data.
///
/// This allows a [`Versioned`] type to be passed into
/// [`read_json`](crate::read_json), etc.
impl<T: Versioned + DeserializeOwned> VersionSet for T {
    type Latest = T;
    type Context = ();
    const VERSIONS: &'static [u32] = &[T::VERSION];

    // `version` is always `T::VERSION`.
    fn parse<'de, D: Deserializer<'de>>(
        _version: u32,
        deserializer: D,
    ) -> Result<Self, D::Error> {
        T::deserialize(deserializer)
    }

    fn version(&self) -> u32 {
        T::VERSION
    }

    fn step(self, _cx: &()) -> Step<Self> {
        Step::Done(self)
    }
}

/// A convenience macro to define a [`VersionSet`] out of [`From`] and [`TryFrom`] implementations.
///
/// # Examples
///
/// Here, there are three config versions. The conversion from version 1 to 2 is
/// infallible, but 2 to 3 can fail.
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use oxide_versioned_envelope::{
///     Versioned, WriteEnvelope, errors::ReadError, read_json, version_set,
/// };
/// use serde::{Deserialize, Serialize};
/// use std::{
///     error::Error as _,
///     net::{AddrParseError, SocketAddr},
/// };
///
/// #[derive(Debug, Deserialize, Serialize)]
/// struct ConfigV1 {
///     name: String,
///     // Version 1 stored just a port.
///     port: u16,
/// }
///
/// #[derive(Debug, Deserialize, Serialize)]
/// struct ConfigV2 {
///     name: String,
///     // Version 2 replaced the port with a listen address
///     // and possible DNS lookups.
///     listen: String,
/// }
///
/// #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
/// struct ConfigV3 {
///     name: String,
///     // In version 3 it was decided that DNS lookups were
///     // not allowed in this path, so the address is now a
///     // `SocketAddr`.
///     listen: SocketAddr,
/// }
///
/// impl Versioned for ConfigV1 {
///     const VERSION: u32 = 1;
/// }
///
/// impl Versioned for ConfigV2 {
///     const VERSION: u32 = 2;
/// }
///
/// impl Versioned for ConfigV3 {
///     const VERSION: u32 = 3;
/// }
///
/// // This conversion is infallible.
/// impl From<ConfigV1> for ConfigV2 {
///     fn from(v1: ConfigV1) -> Self {
///         Self { name: v1.name, listen: format!("0.0.0.0:{}", v1.port) }
///     }
/// }
///
/// #[derive(Debug, thiserror::Error)]
/// #[error(
///     "configuration {name:?} listens on {listen:?}, which is not an IP \
///      address and port"
/// )]
/// struct InvalidListen {
///     name: String,
///     listen: String,
///     #[source]
///     source: AddrParseError,
/// }
///
/// // This conversion can fail.
/// impl TryFrom<ConfigV2> for ConfigV3 {
///     type Error = InvalidListen;
///
///     fn try_from(v2: ConfigV2) -> Result<Self, Self::Error> {
///         match v2.listen.parse() {
///             Ok(listen) => Ok(Self { name: v2.name, listen }),
///             Err(source) => Err(InvalidListen {
///                 name: v2.name,
///                 listen: v2.listen,
///                 source,
///             }),
///         }
///     }
/// }
///
/// // Define a VersionSet for Config.
/// version_set! {
///     #[derive(Debug)]
///     enum Config -> ConfigV3 {
///         // This conversion is infallible.
///         V1(ConfigV1) => V2,
///         // This fallible conversion is marked with `try`.
///         V2(ConfigV2) => try V3,
///         V3(ConfigV3),
///     }
/// }
///
/// // A version 1 configuration always upgrades successfully.
/// let v1 = serde_json::to_vec(&WriteEnvelope::new(&ConfigV1 {
///     name: "api".to_owned(),
///     port: 8080,
/// }))?;
/// assert_eq!(
///     read_json::<Config>(&v1)?.into_value(),
///     ConfigV3 { name: "api".to_owned(), listen: "0.0.0.0:8080".parse()? },
/// );
///
/// // A version 2 configuration with a valid address upgrades.
/// let v2 = serde_json::to_vec(&WriteEnvelope::new(&ConfigV2 {
///     name: "api".to_owned(),
///     listen: "[::1]:8080".to_owned(),
/// }))?;
/// assert_eq!(
///     read_json::<Config>(&v2)?.into_value(),
///     ConfigV3 { name: "api".to_owned(), listen: "[::1]:8080".parse()? },
/// );
///
/// // A version 2 configuration with a hostname does not.
/// let hostname = serde_json::to_vec(&WriteEnvelope::new(&ConfigV2 {
///     name: "api".to_owned(),
///     listen: "localhost:8080".to_owned(),
/// }))?;
/// let error = read_json::<Config>(&hostname)
///     .expect_err("a hostname is not a socket address");
/// let ReadError::Conversion(failure) = &error else {
///     panic!("expected a conversion failure, got: {error}");
/// };
/// assert_eq!((failure.from_version(), failure.to_version()), (2, 3));
/// assert_eq!(
///     error.source().expect("the error carries the cause").to_string(),
///     "configuration \"api\" listens on \"localhost:8080\", which is not an \
///      IP address and port",
/// );
/// # Ok(()) }
/// ```
///
/// For an extended example, see [`examples/version-set-extra.rs`](https://github.com/oxidecomputer/oxide-versioned-envelope/blob/main/crates/oxide-versioned-envelope/examples/version-set-extra.rs).
///
/// ## Examples that fail at compile time
///
/// Versions not in order:
///
/// ```compile_fail,E0080
/// # use oxide_versioned_envelope::{Versioned, version_set};
/// # #[derive(serde::Deserialize)]
/// # struct V1Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V2Payload;
/// # impl Versioned for V1Payload { const VERSION: u32 = 1; }
/// # impl Versioned for V2Payload { const VERSION: u32 = 2; }
/// # impl From<V2Payload> for V1Payload { fn from(_: V2Payload) -> Self { Self } }
/// version_set! {
///     enum NotAscending -> V1Payload {
///         V2(V2Payload) => V1,
///         V1(V1Payload),
///     }
/// }
/// ```
///
/// A conversion step goes backwards:
///
/// ```compile_fail,E0080
/// # use oxide_versioned_envelope::{Versioned, version_set};
/// # #[derive(serde::Deserialize)]
/// # struct V1Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V2Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V3Payload;
/// # impl Versioned for V1Payload { const VERSION: u32 = 1; }
/// # impl Versioned for V2Payload { const VERSION: u32 = 2; }
/// # impl Versioned for V3Payload { const VERSION: u32 = 3; }
/// # impl From<V1Payload> for V2Payload { fn from(_: V1Payload) -> Self { Self } }
/// # impl From<V2Payload> for V1Payload { fn from(_: V2Payload) -> Self { Self } }
/// version_set! {
///     enum Backwards -> V3Payload {
///         V1(V1Payload) => V2,
///         V2(V2Payload) => V1,
///         V3(V3Payload),
///     }
/// }
/// ```
///
/// The last version introduces a loop:
///
/// ```compile_fail,E0080
/// # use oxide_versioned_envelope::{Versioned, version_set};
/// # #[derive(serde::Deserialize)]
/// # struct V1Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V2Payload;
/// # impl Versioned for V1Payload { const VERSION: u32 = 1; }
/// # impl Versioned for V2Payload { const VERSION: u32 = 2; }
/// # impl From<V1Payload> for V2Payload { fn from(_: V1Payload) -> Self { Self } }
/// version_set! {
///     enum NewestSteps -> V2Payload {
///         V1(V1Payload) => V2,
///         V2(V2Payload) => V2,
///     }
/// }
/// ```
///
/// A non-latest version does not convert to the latest version:
///
/// ```compile_fail,E0080
/// # use oxide_versioned_envelope::{Versioned, version_set};
/// # #[derive(serde::Deserialize)]
/// # struct V1Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V2Payload;
/// # #[derive(serde::Deserialize)]
/// # struct V3Payload;
/// # impl Versioned for V1Payload { const VERSION: u32 = 1; }
/// # impl Versioned for V2Payload { const VERSION: u32 = 2; }
/// # impl Versioned for V3Payload { const VERSION: u32 = 3; }
/// # impl From<V1Payload> for V2Payload { fn from(_: V1Payload) -> Self { Self } }
/// # impl From<V2Payload> for V3Payload { fn from(_: V2Payload) -> Self { Self } }
/// version_set! {
///     enum OlderDoesNotStep -> V3Payload {
///         V1(V1Payload) => V2,
///         V2(V2Payload),
///         V3(V3Payload),
///     }
/// }
/// ```
#[macro_export]
macro_rules! version_set {
    // Implementation notes:
    //
    // * The variants are munched one at a time to tell apart
    //   `=> V2`, `=> try V2` and no step.
    // * Each version is normalized into `{ [attrs] Variant (Type) (kind) }`.
    (@munch $header:tt [$($acc:tt)*]) => {
        $crate::version_set!(@emit $header [$($acc)*]);
    };
    (@munch $header:tt [$($acc:tt)*]
        $(#[$vattr:meta])* $var:ident ($ty:ty) => try $target:ident
        $(, $($rest:tt)*)?
    ) => {
        $crate::version_set!(@munch $header
            [$($acc)* { [$(#[$vattr])*] $var ($ty) (try $target) }]
            $($($rest)*)?
        );
    };
    (@munch $header:tt [$($acc:tt)*]
        $(#[$vattr:meta])* $var:ident ($ty:ty) => $target:ident
        $(, $($rest:tt)*)?
    ) => {
        $crate::version_set!(@munch $header
            [$($acc)* { [$(#[$vattr])*] $var ($ty) (next $target) }]
            $($($rest)*)?
        );
    };
    (@munch $header:tt [$($acc:tt)*]
        $(#[$vattr:meta])* $var:ident ($ty:ty)
        $(, $($rest:tt)*)?
    ) => {
        $crate::version_set!(@munch $header
            [$($acc)* { [$(#[$vattr])*] $var ($ty) (done) }]
            $($($rest)*)?
        );
    };

    (@emit [$(#[$attr:meta])* $vis:vis $name:ident $latest:ty]
        [$({ [$(#[$vattr:meta])*] $var:ident ($ty:ty) $kind:tt })+]
    ) => {
        $(#[$attr])*
        $vis enum $name {
            $($(#[$vattr])* $var($ty),)+
        }

        impl $crate::VersionSet for $name {
            type Latest = $latest;

            type Context = ();

            const VERSIONS: &'static [u32] =
                &[$(<$ty as $crate::Versioned>::VERSION),+];

            fn parse<'de, D: $crate::__private::serde_core::Deserializer<'de>>(
                version: u32,
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                $(
                    if version == <$ty as $crate::Versioned>::VERSION {
                        return ::core::result::Result::map(
                            <$ty as $crate::__private::serde_core::Deserialize<'de>>::deserialize(
                                deserializer,
                            ),
                            Self::$var,
                        );
                    }
                )+
                ::core::unreachable!("version {version} is not in VERSIONS")
            }

            fn version(&self) -> u32 {
                match self {
                    $(Self::$var(_) => <$ty as $crate::Versioned>::VERSION,)+
                }
            }

            fn step(self, _cx: &()) -> $crate::Step<Self> {
                $(
                    #[allow(dead_code, non_upper_case_globals)]
                    const $var: u32 = <$ty as $crate::Versioned>::VERSION;
                )+
                match self {
                    $(Self::$var(payload) => $crate::version_set!(@step payload $kind),)+
                }
            }
        }

        const _: () = {
            $(
                #[allow(non_upper_case_globals)]
                const $var: u32 = <$ty as $crate::Versioned>::VERSION;
            )+
            $crate::assert_version_set_well_formed::<$name>();
            $($crate::version_set!(@check $name $var $kind);)+
        };
    };

    (@step $payload:ident (next $target:ident)) => {
        $crate::Step::Next(Self::$target(
            ::core::convert::Into::into($payload),
        ))
    };
    (@step $payload:ident (try $target:ident)) => {
        match ::core::convert::TryInto::try_into($payload) {
            ::core::result::Result::Ok(next) => {
                $crate::Step::Next(Self::$target(next))
            }
            ::core::result::Result::Err(error) => {
                $crate::Step::Failed {
                    to: $target,
                    source: ::std::boxed::Box::new(error),
                }
            }
        }
    };
    (@step $payload:ident (done)) => {
        $crate::Step::Done(::core::convert::Into::into($payload))
    };

    // A set of extra checks that can be done with this macro.
    (@check $name:ident $var:ident (next $target:ident)) => {
        $crate::version_set!(@check_forward $name $var $target);
    };
    (@check $name:ident $var:ident (try $target:ident)) => {
        $crate::version_set!(@check_forward $name $var $target);
    };
    (@check $name:ident $var:ident (done)) => {
        ::core::assert!(
            {
                let versions =
                    <$name as $crate::VersionSet>::VERSIONS;
                $var == versions[versions.len() - 1]
            },
            ::core::concat!(
                "in `version_set!`, `",
                ::core::stringify!($name),
                "::",
                ::core::stringify!($var),
                "` has no step, but only the newest variant may end the ",
                "upgrade: add `=> Newer` to name the variant it converts to",
            )
        );
    };
    (@check_forward $name:ident $var:ident $target:ident) => {
        ::core::assert!(
            $var < $target,
            ::core::concat!(
                "in `version_set!`, `",
                ::core::stringify!($name),
                "::",
                ::core::stringify!($var),
                " => ",
                ::core::stringify!($target),
                "` does not go to a newer version: a step must name a ",
                "variant listed after it",
            )
        );
    };

    (
        $(#[$attr:meta])*
        $vis:vis enum $name:ident -> $latest:ty {
            $($body:tt)*
        }
    ) => {
        $crate::version_set!(@munch [$(#[$attr])* $vis $name $latest] [] $($body)*);
    };
}
