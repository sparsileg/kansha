// Small line icons for the navigation bar: SVG path data on a 16x16 grid.
// Every button also has a text label; the icon never carries meaning alone.

const ICONS: Record<string, string> = {
  home: "M2 8l6-5 6 5M4 7v6.5h8V7",
  "tools.reminders": "M4 11V7a4 4 0 018 0v4l1.2 1.2H2.8zM6.5 14a1.5 1.5 0 003 0",
  "tools.calendar": "M2 6.5h12M5 1.5v3M11 1.5v3M3 3h10a1 1 0 011 1v9a1 1 0 01-1 1H3a1 1 0 01-1-1V4a1 1 0 011-1z",
  "tools.reconcile": "M3 8.5l3.5 3.5L13 4.5",
  "view.investments": "M2 13.5V2.5M2 13.5h12M5 10.5l3-3 2 2 4-5",
  "tools.accounts": "M2.5 4h11M2.5 8h11M2.5 12h11",
  account: "M2 6l6-3.5L14 6M3.5 7v5M6.5 7v5M9.5 7v5M12.5 7v5M2 13.5h12",
  tag: "M2 8.5V3h5.5L14 9.5 9.5 14zM5 5.5h.01",
  report: "M4 2h6l3 3v9H4zM6.5 8.5h4M6.5 11h4",
  gear: "M8 5.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5zM8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2",
  dot: "M8 5.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5z",
};

export function navIcon(id: string): string {
  if (ICONS[id]) return ICONS[id];
  if (id.startsWith("account:")) return ICONS.account;
  if (id === "tools.payees" || id === "tools.categories" || id === "tools.tags") return ICONS.tag;
  if (id.startsWith("reports.")) return ICONS.report;
  if (id.startsWith("edit.")) return ICONS.gear;
  return ICONS.dot;
}
