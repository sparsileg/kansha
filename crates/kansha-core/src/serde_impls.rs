//! serde and specta support for core value types, used for audit
//! before/after snapshots (AUD-010) and for IPC. Values cross as their
//! canonical strings, so money never passes through a JSON float (NFR-030).

use std::borrow::Cow;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::date::{Date, Timestamp};
use crate::money::{Money, Price, Quantity, Rate};

macro_rules! string_form {
    ($($t:ty),+) => {$(
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <Cow<'de, str>>::deserialize(d)?;
                <$t>::from_str(&text).map_err(serde::de::Error::custom)
            }
        }

        #[cfg(feature = "specta")]
        impl specta::Type for $t {
            fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
                <String as specta::Type>::definition(types)
            }
        }
    )+};
}

string_form!(Money, Quantity, Price, Rate, Date, Timestamp);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_serialize_as_strings() {
        let m: Money = "-12.30".parse().unwrap();
        assert_eq!(serde_json::to_string(&m).unwrap(), r#""-12.30""#);
        let q: Quantity = "1.5".parse().unwrap();
        assert_eq!(serde_json::to_string(&q).unwrap(), r#""1.5""#);
        let d: Date = "2026-01-31".parse().unwrap();
        assert_eq!(serde_json::to_string(&d).unwrap(), r#""2026-01-31""#);
    }

    #[test]
    fn values_deserialize_from_strings_and_reject_bad_forms() {
        let m: Money = serde_json::from_str(r#""-12.30""#).unwrap();
        assert_eq!(m.cents(), -1230);
        let d: Date = serde_json::from_str(r#""2026-01-31""#).unwrap();
        assert_eq!(d.to_string(), "2026-01-31");
        assert!(serde_json::from_str::<Money>("12.30").is_err()); // a JSON number
        assert!(serde_json::from_str::<Money>(r#""1,000.00""#).is_err());
        assert!(serde_json::from_str::<Date>(r#""2026-02-30""#).is_err());
    }
}
