//! JSON serialization of core value types, used for audit before/after
//! snapshots (AUD-010). Values serialize as their canonical strings, so
//! money never passes through a JSON float (NFR-030).

use serde::{Serialize, Serializer};

use crate::date::{Date, Timestamp};
use crate::money::{Money, Price, Quantity, Rate};

macro_rules! serialize_as_string {
    ($($t:ty),+) => {$(
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
    )+};
}

serialize_as_string!(Money, Quantity, Price, Rate, Date, Timestamp);

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
}
