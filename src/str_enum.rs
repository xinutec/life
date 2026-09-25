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
//! So a variant added without its `FromStr` arm is written happily and then fails
//! every read. A round-trip test over a hand-written `ALL` has the same hole;
//! generating both directions from one table closes it.

/// Declare a string-backed enum, its `ALL`, and both directions of its mapping.
///
/// The name after the `:` appears in parse failures — `unknown location kind
/// "attic"` — which reach the user as a sync push's 400 body.
///
/// Attributes and doc comments pass through.
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
