// The user-facing date format (SET-030): every date shown or typed uses
// it. Logs and histories show timestamps as stored (ISO 8601 UTC). Kept in
// localStorage until the `settings` module lands (SET-070); see prefs.ts.

import { loadPref, savePref } from "./prefs";

export type DateFormat = "mdy" | "dmy" | "ymd";

export const DATE_FORMATS: { value: DateFormat; label: string }[] = [
  { value: "mdy", label: "MM/DD/YYYY" },
  { value: "dmy", label: "DD/MM/YYYY" },
  { value: "ymd", label: "YYYY-MM-DD" },
];

const isFormat = (v: unknown) => v === "mdy" || v === "dmy" || v === "ymd";

class DateFormatState {
  value = $state<DateFormat>(loadPref<DateFormat>("dateFormat", "mdy", isFormat));

  set(format: DateFormat) {
    this.value = format;
    savePref("dateFormat", format);
  }
}

export const dateFormatState = new DateFormatState();
