import { describe, expect, it } from "vitest";
import { addMonths, monthGrid, monthLabel, weekdayNames, weekdayOf } from "./date";

describe("calendar helpers", () => {
  it("weekdayOf: 0 = Sunday", () => {
    expect(weekdayOf("2026-08-01")).toBe(6); // Saturday
    expect(weekdayOf("2026-11-01")).toBe(0); // Sunday
    expect(weekdayOf("1970-01-01")).toBe(4); // Thursday
    expect(weekdayOf("2028-02-29")).toBe(2); // Tuesday
    expect(weekdayOf("nope")).toBe(-1);
  });

  it("addMonths lands on the first of the month, across years", () => {
    expect(addMonths("2026-11-15", 2)).toBe("2027-01-01");
    expect(addMonths("2026-01-31", -1)).toBe("2025-12-01");
    expect(addMonths("2026-09-24", 0)).toBe("2026-09-01");
  });

  it("monthGrid has 42 Sunday-first days around the month", () => {
    const g = monthGrid("2026-08-15");
    expect(g).toHaveLength(42);
    expect(g[0]).toBe("2026-07-26");
    expect(g[6]).toBe("2026-08-01");
    expect(g[41]).toBe("2026-09-05");
    expect(monthGrid("2026-11-10")[0]).toBe("2026-11-01");
  });

  it("monthGrid can start weeks on Monday (SET-030)", () => {
    const g = monthGrid("2026-08-15", 1);
    expect(g[0]).toBe("2026-07-27"); // Monday
    expect(g[5]).toBe("2026-08-01");
    expect(weekdayOf(g[0])).toBe(1);
    // A month starting on Monday starts the grid.
    expect(monthGrid("2026-06-10", 1)[0]).toBe("2026-06-01");
    // Starting on Sunday, a Monday-first grid backs up six days.
    expect(monthGrid("2026-11-10", 1)[0]).toBe("2026-10-26");
    expect(weekdayNames(1)).toEqual(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    expect(weekdayNames(0)[0]).toBe("Sun");
  });

  it("monthLabel", () => {
    expect(monthLabel("2026-09-24")).toBe("September 2026");
  });
});
