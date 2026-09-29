import { mount } from "svelte";
import App from "./App.svelte";
// Global element styles, then every theme: each is scoped by
// [data-theme="…"] on <html>, so switching is instant.
import "./css/base.css";
import "./css/themes/light.css";
import "./css/themes/dark.css";
import "./css/themes/classic.css";
import { applyTheme, themeState } from "./lib/state/theme.svelte";

// Before the first paint, so the window never shows unthemed.
applyTheme(document.documentElement, themeState.theme, themeState.fontSize);

const target = document.getElementById("app");
if (!target) {
  throw new Error("missing #app root element");
}

export default mount(App, { target });
