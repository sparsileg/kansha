import { describe, expect, it } from "vitest";
import { assetClassLabel, classText } from "./securityTypes";

describe("asset classes", () => {
  it("names the four bond classes", () => {
    expect(assetClassLabel("bond_short")).toBe("Short-term bonds");
    expect(assetClassLabel("bond_intl")).toBe("Intl bonds");
  });

  it("shows a mix as its classes and percents, else the class", () => {
    expect(classText({ asset_class: "cash", class_mix: [] })).toBe("Cash");
    expect(
      classText({
        asset_class: "us_equity",
        class_mix: [
          { asset_class: "us_equity", percent: "60" },
          { asset_class: "bond_intermediate", percent: "40" },
        ],
      }),
    ).toBe("US equity 60%, Intermed bonds 40%");
  });
});
