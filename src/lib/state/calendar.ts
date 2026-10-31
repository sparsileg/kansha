// What the Calendar keeps while it is closed (CAL-010). Opening another
// book reloads the page, which clears it.

class CalendarState {
  /** The first day of the month last shown; `null` before any. */
  month: string | null = null;
}

export const calendarState = new CalendarState();
