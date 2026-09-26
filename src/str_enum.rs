//! One declaration for an enum stored and sent as a short string.
//!
//! Hand-written, `Display` matches on `Self` and breaks the build on a new
//! variant, but `FromStr` ends in `other => Err(..)` and compiles, so the new
//! variant is written fine and then fails every read. Generating both from one
//! table closes that hole.

/// Declare a string-backed enum, its `ALL`, and both directions of its mapping.
/// Attributes and doc comments pass through; the enum must be `Copy`.
///
/// The name after the `:` appears in parse errors (`unknown location kind
/// "attic"`), which reach the user as a sync push's 400 body.
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
