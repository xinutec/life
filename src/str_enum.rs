//! One declaration for an enum stored and sent as a short string, so `FromStr`
//! cannot fall behind `Display` and fail every read of a new variant.

/// Declare a string-backed `Copy` enum, its `ALL`, both mappings and its database
/// mapping ([`varchar_sql!`](crate::varchar_sql)). The name after `:` appears in
/// parse errors, which reach the user as a push's 400 body.
/// ```ignore
/// str_enum! { pub enum LocationKind: "location kind" { House => "house", Room => "room" } }
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
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

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

        $crate::varchar_sql!($name);
    };
}

/// `#[derive(sqlx::Type)]` would declare a SQL `ENUM` and fail on real rows.
/// Decoding parses, so a stored value outside the type fails the query.
#[macro_export]
macro_rules! varchar_sql {
    ($t:ty) => {
        impl sqlx::Type<sqlx::MySql> for $t {
            fn type_info() -> <sqlx::MySql as sqlx::Database>::TypeInfo {
                <str as sqlx::Type<sqlx::MySql>>::type_info()
            }
            fn compatible(ty: &<sqlx::MySql as sqlx::Database>::TypeInfo) -> bool {
                <str as sqlx::Type<sqlx::MySql>>::compatible(ty)
            }
        }

        impl<'q> sqlx::Encode<'q, sqlx::MySql> for $t {
            fn encode_by_ref(
                &self,
                buf: &mut <sqlx::MySql as sqlx::Database>::ArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as sqlx::Encode<'q, sqlx::MySql>>::encode_by_ref(&self.as_str(), buf)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::MySql> for $t {
            fn decode(
                value: <sqlx::MySql as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                <&str as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?
                    .parse()
                    .map_err(Into::into)
            }
        }
    };
}
