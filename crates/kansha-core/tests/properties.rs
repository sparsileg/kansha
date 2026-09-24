//! Property-based tests (TEST-080). Phase 0: money/quantity round-trips.

use kansha_core::{Money, Quantity};
use proptest::prelude::*;

proptest! {
    #[test]
    fn money_text_round_trips(cents in any::<i64>()) {
        let m = Money::from_cents(cents);
        prop_assert_eq!(m.to_string().parse::<Money>().unwrap(), m);
    }

    #[test]
    fn quantity_text_round_trips(raw in any::<i64>()) {
        let q = Quantity::from_raw(raw);
        prop_assert_eq!(q.to_string().parse::<Quantity>().unwrap(), q);
    }

    #[test]
    fn money_sum_is_order_independent(
        mut v in prop::collection::vec(-1_000_000_000_i64..1_000_000_000, 0..50)
    ) {
        let forward: Money = v.iter().copied().map(Money::from_cents).sum();
        v.reverse();
        let backward: Money = v.iter().copied().map(Money::from_cents).sum();
        prop_assert_eq!(forward, backward);
    }

    #[test]
    fn money_decimal_round_trips(cents in any::<i64>()) {
        let m = Money::from_cents(cents);
        prop_assert_eq!(Money::from_decimal(m.to_decimal()).unwrap(), m);
    }
}
