import { mount } from "svelte";
import App from "./App.svelte";
// Global element styles, then every theme: each is scoped by
// [data-theme="…"] on <html>, so switching is instant.
import "./css/base.css";
import "./css/themes/light.css";
import "./css/themes/dark.css";
import "./css/themes/classic.css";
import "./css/themes/matrix.css";
import "./css/themes/nordic.css";
import { applyTheme, themeState } from "./lib/state/theme.svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("missing #app root element");
}

// This computer's theme, font, and size (SET-070) before the first paint, so the
// window never shows unthemed; the defaults if they cannot be read.
applyTheme(document.documentElement, themeState.theme, themeState.fontSize, themeState.font);
void themeState.load().finally(() => {
  applyTheme(document.documentElement, themeState.theme, themeState.fontSize, themeState.font);
  mount(App, { target });
});
