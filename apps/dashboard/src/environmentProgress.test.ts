import { describe, expect, it } from "vitest";

import { environmentProgress } from "./environmentProgress";

describe("environmentProgress", () => {
  it("extracts a bounded callback percentage", () => {
    expect(environmentProgress("browser-open · Downloading · 37%")).toBe(37);
    expect(environmentProgress("browser-open · 100% · Started")).toBe(100);
  });

  it("ignores missing or invalid percentages", () => {
    expect(environmentProgress("browser-open-success")).toBeNull();
    expect(environmentProgress("browser-open · 101% · Invalid")).toBeNull();
  });
});
