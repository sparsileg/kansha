// Open reports. Each report is a window (windows.svelte.ts) with its own
// settings, built report, collapsed groups, and the saved report it came
// from (RPT-020). Reports are built in Rust; this only asks for them and
// remembers view state.

import { call, commands } from "../api";
import { PERIOD_PRESETS, presetLabel } from "../reports/meta";
import { statusState } from "./status.svelte";
import type {
  PageOrientation,
  PeriodChoice,
  ReportFolder,
  ReportKind,
  ReportSettings,
  Report,
  SavedReport,
} from "../types/bindings";
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
  /** Monthly, Quarterly, Yearly: the periods to pick from, newest
   * first (from Rust). Empty for other presets. */
  periods = $state<PeriodChoice[]>([]);
  /** The saved report being shown, if it is one. */
  saved = $state<SavedReport | null>(null);
  /** Paths ("0/2/1") of collapsed groups. */
  collapsed = $state<Set<string>>(new Set());
  loading = $state(false);
  error = $state<string | null>(null);
  /** The graph or the table folded away (view only, not saved). */
  hideGraph = $state(false);
  hideTable = $state(false);
  /** Closing asked to save a new report: the window shows the Save
   * dialog, then closes. */
  saveOnClose = $state(false);
  /** The menu asked for a fresh copy of this new, changed report and
   * the user chose Save: the window shows the Save dialog, then the
   * fresh copy opens in a window of its own. */
  saveThenOpen = $state(false);
  /** Opened from the Reports menu with the standard settings (not
   * saved, not a drill-down): the menu replaces it (RPT-020). */
  fromMenu = false;
  /** Page orientation last chosen in the Save PDF dialog. */
  orientation = $state<PageOrientation>("portrait");
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

  /** A saved report's name; else "Net Worth - Year to date", and a
   * custom range shows only the title. */
  get heading(): string {
    if (this.saved) return this.saved.name;
    const { preset: p, from } = this.settings.range;
    if (p === "custom") return this.settings.title;
    if (PERIOD_PRESETS.includes(p)) {
      const period = this.periods.find((c) => c.from === from) ?? (from ? undefined : this.periods[0]);
      if (period) return `${this.settings.title} - ${period.label}`;
    }
    return `${this.settings.title} - ${presetLabel(p)}`;
  }

  /** The settings differ from how the report opened or was last saved. */
  get dirty(): boolean {
    return fingerprint($state.snapshot(this.settings)) !== this.#baseline;
  }

  /** New settings from the Customize dialog or the toolbar. */
  async apply(settings: ReportSettings): Promise<void> {
    this.settings = settings;
    this.collapsed = new Set();
    await this.run();
  }

  async run(): Promise<void> {
    const seq = ++this.#seq;
    this.loading = true;
    try {
      const range = $state.snapshot(this.settings.range);
      const [report, periods] = await Promise.all([
        call(commands.reportRun($state.snapshot(this.settings))),
        PERIOD_PRESETS.includes(range.preset) ? call(commands.reportPeriodChoices(range)) : [],
      ]);
      if (seq !== this.#seq) return;
      this.report = report;
      this.periods = periods;
      this.error = null;
    } catch (e) {
      if (seq !== this.#seq) return;
      this.report = null;
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  /** Show other settings in this window, as if newly opened. */
  async replace(settings: ReportSettings, saved: SavedReport | null): Promise<void> {
    this.settings = settings;
    this.saved = saved;
    this.#baseline = fingerprint(settings);
    this.collapsed = new Set();
    this.report = null;
    await this.run();
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
    void reportState.refreshSaved().catch(() => {});
    return saved;
  }

  async exportCsv(): Promise<void> {
    statusState.show(`Saved to ${await call(commands.reportExportCsv($state.snapshot(this.settings)))}`);
  }

  /** Save the page as a PDF (opened in the PDF viewer, which prints). */
  async savePdf(): Promise<void> {
    statusState.show(`Saved to ${await call(commands.reportSavePdf(this.heading, this.orientation))}`);
  }
}

class ReportsState {
  #open = new Map<number, ReportInstance>();
  /** The Manage Saved Reports dialog is open. */
  savedOpen = $state(false);
  /** Saved report folders and saved reports, each by name, for the
   * Reports > Saved Reports menu and the Manage dialog. */
  folders = $state<ReportFolder[]>([]);
  savedList = $state<SavedReport[]>([]);

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

  /** A report with its standard settings (the Reports menu): in place
   * of the open menu copy of that report, else in a new window. */
  async open(kind: ReportKind): Promise<ReportInstance> {
    const settings = await commands.reportDefaults(kind);
    const old = [...this.#open.values()].find((i) => i.fromMenu && i.saved === null && i.kind === kind);
    if (old) return this.#replace(old, settings, null);
    const inst = await this.openWith(settings);
    inst.fromMenu = true;
    return inst;
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

  /** Reload the folders and saved reports. */
  async refreshSaved(): Promise<void> {
    const [folders, saved] = await Promise.all([call(commands.reportFolderList()), call(commands.savedReportList())]);
    this.folders = folders;
    this.savedList = saved;
    // Open copies follow a rename; a deleted one becomes unnamed.
    for (const inst of this.#open.values()) {
      if (inst.saved) inst.saved = saved.find((r) => r.id === inst.saved?.id) ?? null;
    }
  }

  /** A saved report by ID (the Reports > Saved Reports menu). */
  async openSavedId(id: number): Promise<ReportInstance | null> {
    const saved = this.savedList.find((r) => r.id === id);
    return saved ? this.openSaved(saved) : null;
  }

  /** A saved report: in place of its open copy, else in a new window. */
  async openSaved(saved: SavedReport): Promise<ReportInstance> {
    // `saved` may be reactive (a proxy), which structuredClone rejects.
    const settings = structuredClone($state.snapshot(saved.settings));
    const old = [...this.#open.values()].find((i) => i.saved?.id === saved.id);
    if (old) return this.#replace(old, settings, saved);
    return this.openWith(settings, saved);
  }

  /** Reopen `inst` with `settings`, asking first to save its changes.
   * Cancel, or Save of a report with no name yet, leaves it as is; the
   * latter opens the new copy after the Save dialog. */
  async #replace(inst: ReportInstance, settings: ReportSettings, saved: SavedReport | null): Promise<ReportInstance> {
    if (inst.dirty) {
      const choice = await confirmState.choose(`Save changes to "${inst.heading}"?`, ["Save", "Don't Save"]);
      if (choice === "Save" && inst.saved) {
        try {
          const fresh = await inst.save(inst.saved.name, false);
          // Reopening the same saved report: its new settings, not the
          // ones it was asked for with.
          if (saved?.id === fresh.id) {
            saved = fresh;
            settings = structuredClone($state.snapshot(fresh.settings));
          }
        } catch (e) {
          inst.error = e instanceof Error ? e.message : String(e);
          windowState.show(inst.id);
          return inst;
        }
      } else if (choice === "Save") {
        inst.saveThenOpen = true;
        windowState.show(inst.id);
        return inst;
      } else if (choice !== "Don't Save") {
        windowState.show(inst.id);
        return inst;
      }
    }
    windowState.show(inst.id);
    await inst.replace(settings, saved);
    return inst;
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
