// The report on screen: its settings, the built report, which groups are
// collapsed, and the saved report it came from (RPT-020). Reports are
// built in Rust; this only asks for them and remembers view state.

import { call, commands } from "../api";
import type {
  ReportKind,
  ReportSettings,
  Report,
  SavedReport,
} from "../types/bindings";

class ReportState {
  settings = $state<ReportSettings | null>(null);
  report = $state<Report | null>(null);
  /** The saved report being shown, if it is one. */
  saved = $state<SavedReport | null>(null);
  /** Paths ("0/2/1") of collapsed groups. */
  collapsed = $state<Set<string>>(new Set());
  loading = $state(false);
  error = $state<string | null>(null);
  /** The Saved Reports dialog is open. */
  savedOpen = $state(false);
  /** Last CSV export's file, to tell the user where it went. */
  exported = $state<string | null>(null);
  #seq = 0;

  get kind(): ReportKind | null {
    return this.settings?.kind ?? null;
  }

  /** A report with its standard settings. */
  async open(kind: ReportKind): Promise<void> {
    const settings = await commands.reportDefaults(kind);
    this.saved = null;
    await this.show(settings);
  }

  /** A report with given settings, e.g. from drilling down. */
  async openWith(settings: ReportSettings): Promise<void> {
    this.saved = null;
    await this.show(settings);
  }

  async openSaved(saved: SavedReport): Promise<void> {
    this.saved = saved;
    await this.show(structuredClone(saved.settings));
  }

  /** New settings from the Customize dialog or the toolbar. */
  async apply(settings: ReportSettings): Promise<void> {
    await this.show(settings);
  }

  async show(settings: ReportSettings): Promise<void> {
    this.settings = settings;
    this.collapsed = new Set();
    this.exported = null;
    await this.run();
  }

  async run(): Promise<void> {
    if (!this.settings) return;
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

  /** Collapse every group below `depth` levels (0 = collapse all). */
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
    if (!this.settings) throw new Error("No report is open.");
    const settings = $state.snapshot(this.settings);
    const saved =
      this.saved && !asNew
        ? await call(commands.savedReportUpdate(this.saved.id, name, settings))
        : await call(commands.savedReportCreate(name, settings));
    this.saved = saved;
    return saved;
  }

  async exportCsv(): Promise<void> {
    if (!this.settings) return;
    this.exported = await call(commands.reportExportCsv($state.snapshot(this.settings)));
  }
}

export const reportState = new ReportState();
