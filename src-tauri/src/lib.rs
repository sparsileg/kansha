//! Tauri shell: command handlers only (spec §17.1). Every handler here
//! maps arguments, calls into `kansha-core`, and maps errors — no
//! financial logic lives in this crate.
//!
//! Commands are registered through [`tauri_specta`], which also generates
//! the TypeScript bindings consumed by `src/lib/api/` (D-120: pinned to
//! `tauri-specta`/`specta` `2.0.0-rc.25` — the newest versions compatible
//! with Tauri v2 at the time of writing; both are release candidates, so a
//! version bump should be treated as a breaking change and re-verified).

use tauri_specta::{Builder, collect_commands};

mod commands;

/// One place that lists every command exposed to the frontend, so the
/// Tauri registration and the TypeScript export can never drift apart.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![commands::app_version])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Regenerate `src/lib/types/bindings.ts` on every debug build. Release
    // builds skip this: the checked-in file is what ships. The path is
    // anchored to this crate's own directory (not the process cwd), since
    // `cargo run`/`tauri dev` can be invoked from either the repo root or
    // `src-tauri/`.
    #[cfg(debug_assertions)]
    {
        let bindings_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types/bindings.ts");
        builder
            .export(specta_typescript::Typescript::default(), bindings_path)
            .expect("failed to export TypeScript bindings");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Kansha application");
}
