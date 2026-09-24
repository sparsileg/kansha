//! Load the synthetic dataset (D-130, TEST-110). Prototype only.

use kansha_core::Origin;
use kansha_core::sample::{self, SampleSpec, SampleSummary};
use tauri::State;

use crate::state::{AppState, CmdResult};

/// Fill an empty book with about three years of synthetic data ending
/// three weeks from today, so the register shows the today line. Refuses a
/// book that already has accounts.
#[tauri::command]
#[specta::specta]
pub fn sample_data_load(state: State<'_, AppState>, seed: u32) -> CmdResult<SampleSummary> {
    let spec = SampleSpec::around(u64::from(seed), state.today())?;
    state.write_as(Origin::System, |tx| sample::generate(tx, &spec))
}
