// Injected by Playwright before the app's own scripts (#63). Stands in for
// Tauri's window internals: app commands go to the dev-only bridge in the
// running Kansha (src-tauri/src/bridge.rs); window and event plugin calls
// are answered here, since a browser page has no native window.
// `window.__KANSHA_BRIDGE__ = { url, token }` is set first.
(() => {
  const { url, token } = window.__KANSHA_BRIDGE__;
  let nextId = 1;
  const callbacks = new Map();

  function transformCallback(callback, once = false) {
    const id = nextId++;
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      return callback && callback(data);
    });
    return id;
  }

  // Plugin calls a page makes for its window: listen for events, set the
  // title, check focus. Nothing native to do in a browser.
  function plugin(cmd) {
    const name = cmd.slice("plugin:".length);
    if (name === "event|listen") return nextId++;
    if (name === "window|is_focused") return true;
    if (name === "window|is_visible") return true;
    return null;
  }

  // Window-only commands open native dialogs or touch the real window.
  const REFUSED = new Set([
    "pick_folder", "pick_backup_file", "pick_book_file", "pick_entries_file",
    "pick_import_file", "report_save_pdf", "help_open_guide",
  ]);

  async function invoke(cmd, args = {}) {
    if (cmd.startsWith("plugin:")) return plugin(cmd);
    if (cmd === "window_mode") return null;
    // A script can answer a file or folder picker:
    // window.__KANSHA_PICKS__ = { pick_import_file: "/path/file.qif" }.
    const picks = window.__KANSHA_PICKS__ || {};
    if (cmd in picks) return picks[cmd];
    if (REFUSED.has(cmd)) {
      console.warn(`bridge-shim: ${cmd} is not available in screenshot runs`);
      return null;
    }
    const reply = await fetch(`${url}/invoke`, {
      method: "POST",
      headers: { "content-type": "application/json", "x-kansha-token": token },
      body: JSON.stringify({ cmd, args }),
    });
    const body = await reply.json();
    if ("err" in body) throw body.err;
    return body.ok;
  }

  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    unregisterCallback: (id) => callbacks.delete(id),
    convertFileSrc: (path) => path,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    plugins: {},
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
})();
