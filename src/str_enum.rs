//! One declaration for an enum that is stored and sent as a short string.
//!
//! **Why this exists is an asymmetry, not a line count.** A hand-written pair of
//! impls has two halves with different safety, and the unsafe half is silent:
//!
//! ```text
//! impl Display   match on Self  -> exhaustive -> a new variant BREAKS THE BUILD
//! impl FromStr   match on &str  -> `other => Err(..)` -> a new variant COMPILES
//! ```
//!
//! So a variant added without its `FromStr` arm writes to the database happily,
//! and then every later READ of that row fails — this codebase's "an unknown
//! stored value fails the read loudly" rule firing in production, against data
//! already stored, instead of at compile time.
//!
//! ⚠ **A round-trip test over an `ALL` array does not close that**, which is why
//! there isn't one: `ALL` is itself hand-written, so forgetting a variant there
//! is the same bug wearing a different coat. Generating both directions from one
//! table is the only construction in which they cannot disagree.

/// Declare a string-backed enum, its `ALL`, and both directions of its mapping.
///
/// The human name after the `:` is the one that appears in a parse failure —
/// `unknown location kind "attic"`. It is user-facing: these errors surface as
/// the 400 body when a sync push carries a value this server cannot read, so it
/// says what a person would call the field rather than what Rust calls the type.
///
/// Everything else passes through, so the derives, `#[serde]`, `#[ts]` and the
/// per-variant doc comments live where they always did.
///
/// ```ignore
/// str_enum! {
///     /// A node kind in the spatial tree.
///     #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
///     #[serde(rename_all = "snake_case")]
///     #[ts(export)]
///     pub enum LocationKind: "location kind" {
///         House => "house",
///         Room => "room",
///     }
/// }
/// ```
///
/// `as_str` takes `self` by value, so the enum must be `Copy` — every one of
/// these is. A type that isn't gets a compile error rather than a surprise.
#[macro_export]
macro_rules! str_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident: $human:literal {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every variant, in declaration order.
            ///
            /// Generated from the same table as the mapping below, so it cannot
            /// fall behind the way a hand-written list does.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            /// The value stored in the database and sent on the wire.
            pub fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $text ),+ }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = ::std::string::String;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                match s {
                    $( $text => ::std::result::Result::Ok(Self::$variant), )+
                    other => ::std::result::Result::Err(
                        ::std::format!("unknown {} {:?}", $human, other),
                    ),
                }
            }
        }
    };
}
