//! Command line and file entry (CLI-010 … CLI-080).
//!
//! Spending entries arrive as `kansha-entries 1` text: from `kansha add`
//! and `kansha run`, from File > Import entries…, and from phones. They
//! are checked here ([`format`], [`names`]) and kept outside the ledger
//! until approved (CLI-040).

pub mod cli;
pub mod format;
pub mod inbox;
pub mod names;
pub mod review;
pub mod sidecar;

pub use format::{Dates, EntryFile, FileEntry, LineError, NamesFile, Spend, SplitLine};
pub use names::{NameError, NameKind, Names};
