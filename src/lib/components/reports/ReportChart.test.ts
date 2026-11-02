import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import ReportChart from "./ReportChart.svelte";
import type { Chart } from "../../types/bindings";

const chart = (names: string[]): Chart =>
  ({
    dates: ["2026-01-31", "2026-02-28"],
    labels: [],
    series: names.map((name, i) => ({ name, style: i === 0 ? "bar" : "line", values: ["1.00", "2.00"], pos: [5000, 10000] })),
    ticks: [
      { label: "$0", pos: 0 },
      { label: "$5M", pos: 10000 },
    ],
    zero: 0,
    x_unit: "month",
  }) as Chart;

describe("ReportChart", () => {
  it("shows the y-axis labels as given, dollar sign and all", () => {
    render(ReportChart, { chart: chart(["Net worth"]) });
    expect(screen.getByText("$5M")).toBeTruthy();
  });

  it("has a legend only with two or more series", () => {
    const one = render(ReportChart, { chart: chart(["Net worth"]) });
    expect(one.container.querySelector("figcaption.legend")).toBeNull();
    one.unmount();
    const two = render(ReportChart, { chart: chart(["Assets", "Net worth"]) });
    const legend = two.container.querySelector("figcaption.legend");
    expect(legend?.textContent).toContain("Assets");
    expect(legend?.textContent).toContain("Net worth");
  });
});
