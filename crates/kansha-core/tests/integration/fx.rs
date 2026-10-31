//! Foreign amounts in the register (REG-036): reading what was typed, the
//! rate reply, and the dollars. No network here.

use kansha_core::date::Date;
use kansha_core::fx;
use kansha_core::testkit::Book;
use kansha_core::{Money, settings};

fn day(s: &str) -> Date {
    s.parse().unwrap()
}

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

const REPLY: &str = r#"{"amount":1.0,"base":"EUR","date":"2026-10-23","rates":{"USD":1.1193}}"#;

fn rate(body: &str, code: &str) -> fx::Quote {
    fx::parse_reply(body, code).unwrap()
}

fn convert(text: &str, q: &fx::Quote, on: &str) -> fx::Converted {
    fx::convert(&fx::parse(text).unwrap(), q, day(on)).unwrap()
}

#[test]
fn reads_an_amount_and_a_code_in_any_case() {
    for text in [
        "85 eur",
        "85 EUR",
        "85eur",
        " 85  Eur ",
        "60+25 eur",
        "170/2 eur",
    ] {
        let f = fx::parse(text).unwrap();
        assert_eq!(f.currency, "EUR", "{text}");
        assert_eq!(f.amount, "85".parse().unwrap(), "{text}");
    }
}

#[test]
fn refuses_what_is_not_an_amount_and_a_code() {
    for (text, why) in [
        ("85", "a currency code"),
        ("eur", "enter an amount"),
        ("85 eu", "\"EU\" is not a currency code"),
        ("85 euro", "\"EURO\" is not a currency code"),
        ("85 e1r", "\"R\" is not a currency code"),
        ("1/0 eur", "divide by zero"),
    ] {
        let err = fx::parse(text).unwrap_err().to_string();
        assert!(err.contains(why), "{text}: {err}");
    }
}

#[test]
fn the_rate_is_read_as_written_and_never_through_a_float() {
    let q = rate(REPLY, "EUR");
    assert_eq!(q.rate, "1.1193".parse().unwrap());
    assert_eq!(q.date, day("2026-10-23"));
    // Seven decimals: a yen rate keeps them all.
    let yen = rate(r#"{"date":"2026-10-23","rates":{"USD":0.0063774}}"#, "JPY");
    assert_eq!(yen.rate, "0.0063774".parse().unwrap());
    assert_eq!(
        convert("100000 jpy", &yen, "2026-10-23").amount,
        m("637.74")
    );
}

#[test]
fn a_bad_reply_is_an_error() {
    for body in [
        "",
        "{}",
        r#"{"date":"2026-10-23","rates":{}}"#,
        r#"{"date":"2026-10-23","rates":{"USD":0}}"#,
        r#"{"date":"2026-10-23","rates":{"USD":-1.2}}"#,
        r#"{"date":"nope","rates":{"USD":1.2}}"#,
    ] {
        assert!(fx::parse_reply(body, "EUR").is_err(), "{body}");
    }
}

#[test]
fn dollars_round_half_even_once_at_the_end() {
    let q = |r: &str| fx::Quote {
        rate: r.parse().unwrap(),
        date: day("2026-10-23"),
    };
    // Exactly half a cent, both ways.
    assert_eq!(
        convert("1 eur", &q("1.125"), "2026-10-23").amount,
        m("1.12")
    );
    assert_eq!(
        convert("1 eur", &q("1.135"), "2026-10-23").amount,
        m("1.14")
    );
    // The sum is not rounded first: 0.005 + 0.005 at 1 is 0.01, not 0.02.
    assert_eq!(
        convert("0.005+0.005 eur", &q("1"), "2026-10-23").amount,
        m("0.01")
    );
    assert_eq!(
        convert("85 eur", &q("1.1193"), "2026-10-23").amount,
        m("95.14")
    );
    assert_eq!(
        convert("-85 eur", &q("1.1193"), "2026-10-23").amount,
        m("-95.14")
    );
}

#[test]
fn a_sum_and_its_total_give_the_same_dollars() {
    let q = rate(REPLY, "EUR");
    assert_eq!(
        convert("60+25 eur", &q, "2026-10-23"),
        convert("85 eur", &q, "2026-10-23")
    );
}

#[test]
fn the_memo_holds_the_amount_and_rate_and_a_rate_from_another_day() {
    let q = rate(REPLY, "EUR");
    assert_eq!(
        convert("85 eur", &q, "2026-10-23").memo,
        "EUR 85.00 @ 1.1193"
    );
    assert_eq!(
        convert("85.125 eur", &q, "2026-10-23").memo,
        "EUR 85.125 @ 1.1193"
    );
    // A Saturday, and a date past today: the rate is an earlier day's.
    assert_eq!(
        convert("85 eur", &q, "2026-10-24").memo,
        "EUR 85.00 @ 1.1193 (2026-10-23)"
    );
}

#[test]
fn dollars_need_no_rate() {
    let f = fx::parse("85.5 usd").unwrap();
    assert!(f.is_dollars());
    let c = fx::dollars(&f).unwrap();
    assert_eq!(c.amount, m("85.50"));
    assert_eq!(c.memo, "");
}

#[test]
fn a_rate_is_never_asked_for_a_day_after_today() {
    let today = day("2026-10-30");
    assert_eq!(fx::rate_day(day("2026-10-29"), today), day("2026-10-29"));
    assert_eq!(fx::rate_day(day("2027-01-01"), today), today);
}

#[test]
fn the_request_names_the_currency_and_day_and_nothing_else() {
    assert_eq!(
        fx::url("EUR", day("2026-10-23")),
        "https://api.frankfurter.dev/v1/2026-10-23?base=EUR&symbols=USD"
    );
}

#[test]
fn lookup_is_off_until_the_setting_is_on() {
    let mut book = Book::new(day("2026-10-30")).unwrap();
    let err = fx::require_allowed(book.conn()).unwrap_err().to_string();
    assert!(err.contains("turn it on in Settings"), "{err}");
    book.write(|tx| {
        let mut s = settings::load(tx.conn())?;
        s.currency_lookup = true;
        settings::save(tx, &s)
    })
    .unwrap();
    fx::require_allowed(book.conn()).unwrap();
}
