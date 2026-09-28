// Open reports. Each report is a window (windows.svelte.ts) with its own
// settings, built report, collapsed groups, and the saved report it came
// from (RPT-020). Reports are built in Rust; this only asks for them and
// remembers view state.

import { call, commands } from "../api";
import { presetLabel } from "../reports/meta";
import type { ReportKind, ReportSettings, Report, SavedReport } from "../types/bindings";
import { confirmState } from "./confirm.svelte";
import { windowState } from "./windows.svelte";

export const REPORT_WINDOW = "report";

/** Settings as text with keys in order, to tell whether they changed. */
const fingerprint = (v: unknown): string =>
  JSON.stringify(v, (_k, x: unknown) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(Object.entries(x as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : x,
  );

export class ReportInstance {
  readonly id: number;
  settings = $state() as ReportSettings;
  report = $state<Report | null>(null);
  /** The saved report being shown, if it is one. */
  saved = $state<SavedReport | null>(null);
  /** Paths ("0/2/1") of collapsed groups. */
  collapsed = $state<Set<string>>(new Set());
  loading = $state(false);
  error = $state<string | null>(null);
  /** Last CSV export's file, to tell the user where it went. */
  exported = $state<string | null>(null);
  /** The graph or the table folded away (view only, not saved). */
  hideGraph = $state(false);
  hideTable = $state(false);
  /** Closing asked to save a new report: the window shows the Save
   * dialog, then closes. */
  saveOnClose = $state(false);
  /** The settings as opened or last saved. */
  #baseline = $state("");
  #seq = 0;

  constructor(id: number, settings: ReportSettings, saved: SavedReport | null) {
    this.id = id;
    this.settings = settings;
    this.saved = saved;
    this.#baseline = fingerprint(settings);
  }

  get kind(): ReportKind {
    return this.settings.kind;
  }

  /** "Net Worth - Year to date"; a custom range shows only the title. */
  get heading(): string {
    const p = this.settings.range.preset;
    return p === "custom" ? this.settings.title : `${this.settings.title} - ${presetLabel(p)}`;
  }

  /** The settings differ from how the report opened or was last saved. */
  get dirty(): boolean {
    return fingerprint($state.snapshot(this.settings)) !== this.#baseline;
  }

  /** New settings from the Customize dialog or the toolbar. */
  async apply(settings: ReportSettings): Promise<void> {
    this.settings = settings;
    this.collapsed = new Set();
    this.exported = null;
    await this.run();
  }

  async run(): Promise<void> {
    const seq = ++this.#seq;
    this.loading = true;
    try {
      const report = await call(commands.reportRun($state.snapshot(this.settings)));
      if (seq !== this.#seq) return;
      this.report = report;
      this.error = null;
    } catch (e) {
      if (seq !== this.#seq) return;
      this.report = null;
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  isCollapsed(path: string): boolean {
    return this.collapsed.has(path);
  }

  toggle(path: string): void {
    const next = new Set(this.collapsed);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    this.collapsed = next;
  }

  collapseAll(): void {
    const paths = new Set<string>();
    const walk = (rows: Report["rows"], prefix: string) =>
      rows.forEach((r, i) => {
        const p = prefix ? `${prefix}/${i}` : String(i);
        if (r.children.length) {
          paths.add(p);
          walk(r.children, p);
        }
      });
    if (this.report) walk(this.report.rows, "");
    this.collapsed = paths;
  }

  expandAll(): void {
    this.collapsed = new Set();
  }

  /** Save under a new name (Save As), or replace the open saved report. */
  async save(name: string, asNew: boolean): Promise<SavedReport> {
    const settings = $state.snapshot(this.settings);
    const saved =
      this.saved && !asNew
        ? await call(commands.savedReportUpdate(this.saved.id, name, settings))
        : await call(commands.savedReportCreate(name, settings));
    this.saved = saved;
    this.#baseline = fingerprint(settings);
    return saved;
  }

  async exportCsv(): Promise<void> {
    this.exported = await call(commands.reportExportCsv($state.snapshot(this.settings)));
  }
}

class ReportsState {
  #open = new Map<number, ReportInstance>();
  /** The Saved Reports dialog is open. */
  savedOpen = $state(false);

  constructor() {
    windowState.register(REPORT_WINDOW, {
      beforeClose: (id) => this.#beforeClose(id),
      closed: (id) => this.#open.delete(id),
    });
  }

  get(id: number): ReportInstance | undefined {
    return this.#open.get(id);
  }

  /** The report on top, if a report window is showing. */
  get current(): ReportInstance | null {
    const id = windowState.shown;
    return id === null ? null : (this.#open.get(id) ?? null);
  }

  /** A report with its standard settings, in a new window. */
  async open(kind: ReportKind): Promise<ReportInstance> {
    return this.openWith(await commands.reportDefaults(kind));
  }

  /** A report with given settings (e.g. from drilling down). */
  async openWith(settings: ReportSettings, saved: SavedReport | null = null): Promise<ReportInstance> {
    let inst: ReportInstance | undefined;
    const id = windowState.add(REPORT_WINDOW, () => inst?.heading ?? "Report");
    inst = new ReportInstance(id, settings, saved);
    this.#open.set(id, inst);
    windowState.show(id);
    return inst;
  }

  async openSaved(saved: SavedReport): Promise<ReportInstance> {
    return this.openWith(structuredClone(saved.settings), saved);
  }

  /** Changed settings: ask to save. A new report is saved by name in its
   * window, which then closes. */
  async #beforeClose(id: number): Promise<boolean> {
    const inst = this.#open.get(id);
    if (!inst || !inst.dirty) return true;
    const choice = await confirmState.choose(`Save changes to "${inst.heading}"?`, ["Save", "Don't Save"]);
    if (choice === "Don't Save") return true;
    if (choice !== "Save") return false;
    if (inst.saved) {
      try {
        await inst.save(inst.saved.name, false);
        return true;
      } catch (e) {
        inst.error = e instanceof Error ? e.message : String(e);
        windowState.show(id);
        return false;
      }
    }
    inst.saveOnClose = true;
    windowState.show(id);
    return false;
  }
}

export const reportState = new ReportsState();
