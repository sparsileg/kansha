// The user-facing date format (SET-030): every date shown or typed uses
// it. Logs and histories show timestamps as stored (ISO 8601 UTC). Kept in
// the book (SET-070) through booksettings.svelte.ts.

import { bookSettings } from "./booksettings.svelte";

export type DateFormat = "mdy" | "dmy" | "ymd";

export const DATE_FORMATS: { value: DateFormat; label: string }[] = [
  { value: "mdy", label: "MM/DD/YYYY" },
  { value: "dmy", label: "DD/MM/YYYY" },
  { value: "ymd", label: "YYYY-MM-DD" },
];

class DateFormatState {
  value = $derived<DateFormat>(bookSettings.value.date_format);

  set(format: DateFormat) {
    void bookSettings.update({ date_format: format });
  }
}

export const dateFormatState = new DateFormatState();
