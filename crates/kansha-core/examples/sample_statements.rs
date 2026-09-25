//! Print mock bank statements for the synthetic dataset, to reconcile a
//! sample book by hand in the app (TEST-110).
//!
//! ```text
//! cargo run -p kansha-core --example sample_statements [today] [seed]
//! ```
//!
//! `today` defaults to the system date and `seed` to 1, matching the
//! app's "Load sample data" on the same day.

use kansha_core::persistence::accounts;
use kansha_core::sample::{self, SampleSpec};
use kansha_core::{Clock, Db, FixedClock, Origin, Result, SystemClock};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let today = match args.next() {
        Some(s) => s.parse()?,
        None => SystemClock.today(),
    };
    let seed = match args.next() {
        Some(s) => s
            .parse()
            .map_err(|_| kansha_core::Error::Invalid(format!("bad seed {s:?}")))?,
        None => 1,
    };
    let clock = FixedClock::new(today);
    let mut db = Db::open_in_memory(&clock)?;
    let spec = SampleSpec::around(seed, today)?;
    db.write(&clock, Origin::System, |tx| sample::generate(tx, &spec))?;
    let statement_date = spec.statement_date()?;

    println!("Sample statements: seed {seed}, today {today}\n");
    for account in accounts::list(db.conn())? {
        let name = &account.fields.name;
        if !["Checking", "Savings", "Visa"].contains(&name.as_str()) {
            continue;
        }
        let stmt = sample::statement(db.conn(), account.id, statement_date)?;
        println!("{name} — statement ending {statement_date}");
        println!(
            "  Opening balance  {:>12}",
            stmt.opening_balance.to_string()
        );
        println!("  Ending balance   {:>12}", stmt.ending_balance.to_string());
        println!("  Posted ({}):", stmt.posted.len());
        for i in &stmt.posted {
            println!(
                "    {}  {:>6}  {:<20} {:>10}",
                i.date,
                i.check_num,
                i.payee_name,
                i.amount.to_string()
            );
        }
        println!("  Not on this statement ({}):", stmt.outstanding.len());
        for i in &stmt.outstanding {
            println!(
                "    {}  {:>6}  {:<20} {:>10}",
                i.date,
                i.check_num,
                i.payee_name,
                i.amount.to_string()
            );
        }
        println!();
    }
    Ok(())
}
