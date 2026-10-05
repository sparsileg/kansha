// The app's version is written in three places; they must agree, since
// backups record it and Help > About shows it.

import { describe, expect, it } from "vitest";

const raw = (glob: Record<string, string>) => Object.values(glob)[0];
const PACKAGE = raw(import.meta.glob<string>("/package.json", { query: "?raw", import: "default", eager: true }));
const TAURI = raw(import.meta.glob<string>("/src-tauri/tauri.conf.json", { query: "?raw", import: "default", eager: true }));
const CARGO = raw(import.meta.glob<string>("/Cargo.toml", { query: "?raw", import: "default", eager: true }));

describe("version", () => {
  it("package.json, tauri.conf.json, and Cargo.toml agree", () => {
    const pkg = (JSON.parse(PACKAGE) as { version: string }).version;
    const tauri = (JSON.parse(TAURI) as { version: string }).version;
    const cargo = CARGO.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    expect({ pkg, tauri, cargo }).toEqual({ pkg, tauri: pkg, cargo: pkg });
  });

  it("is 0.9.1", () => {
    expect((JSON.parse(PACKAGE) as { version: string }).version).toBe("0.9.1");
  });
});
