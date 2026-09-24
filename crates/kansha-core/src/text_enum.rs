//! `text_enum!`: an enum stored as a fixed TEXT value (matching a CHECK
//! list in the schema), serialized to JSON as the same string.

/// Declares an enum whose variants map one-to-one onto TEXT values.
///
/// Generates `as_str`, `ALL`, `FromStr`, `Display`, `serde::Serialize`,
/// and rusqlite `ToSql`/`FromSql`. The strings must match the column's
/// CHECK list in the migrations; `tests/integration/schema.rs` verifies
/// that every value is accepted.
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stored TEXT value.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::error::Error;

            fn from_str(s: &str) -> $crate::error::Result<Self> {
                match s {
                    $($text => Ok($name::$variant),)+
                    _ => Err($crate::error::Error::Parse {
                        kind: stringify!($name),
                        input: s.to_string(),
                        reason: "unknown value".into(),
                    }),
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::std::result::Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl ::rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> ::rusqlite::Result<::rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }

        impl ::rusqlite::types::FromSql for $name {
            fn column_result(
                value: ::rusqlite::types::ValueRef<'_>,
            ) -> ::rusqlite::types::FromSqlResult<Self> {
                let text = value.as_str()?;
                text.parse()
                    .map_err(|e: $crate::error::Error| ::rusqlite::types::FromSqlError::Other(Box::new(e)))
            }
        }
    };
}

pub(crate) use text_enum;
