//! Row ids as distinct types, so one kind cannot be passed for another.

/// A `u64` row id of its own type; a plain number on the wire.
#[macro_export]
macro_rules! row_id {
    ($(#[$m:meta])* $t:ident) => {
        $(#[$m])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
            ::serde::Serialize, ::serde::Deserialize, ::ts_rs::TS,
        )]
        #[ts(type = "number")]
        pub struct $t(pub u64);

        impl ::sqlx::Type<::sqlx::MySql> for $t {
            fn type_info() -> <::sqlx::MySql as ::sqlx::Database>::TypeInfo {
                <u64 as ::sqlx::Type<::sqlx::MySql>>::type_info()
            }
            fn compatible(ty: &<::sqlx::MySql as ::sqlx::Database>::TypeInfo) -> bool {
                <u64 as ::sqlx::Type<::sqlx::MySql>>::compatible(ty)
            }
        }

        impl<'q> ::sqlx::Encode<'q, ::sqlx::MySql> for $t {
            fn encode_by_ref(
                &self,
                buf: &mut <::sqlx::MySql as ::sqlx::Database>::ArgumentBuffer,
            ) -> ::std::result::Result<::sqlx::encode::IsNull, ::sqlx::error::BoxDynError> {
                <u64 as ::sqlx::Encode<'q, ::sqlx::MySql>>::encode_by_ref(&self.0, buf)
            }
        }

        impl<'r> ::sqlx::Decode<'r, ::sqlx::MySql> for $t {
            fn decode(
                value: <::sqlx::MySql as ::sqlx::Database>::ValueRef<'r>,
            ) -> ::std::result::Result<Self, ::sqlx::error::BoxDynError> {
                <u64 as ::sqlx::Decode<'r, ::sqlx::MySql>>::decode(value).map($t)
            }
        }

        impl ::std::fmt::Display for $t {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl ::std::convert::From<u64> for $t {
            fn from(id: u64) -> Self {
                $t(id)
            }
        }
    };
}
