import { describe, expect, it } from "vitest";
import { moveItem } from "./library";

describe("moveItem", () => {
  it("moves an item without mutating the original order", () => {
    const original = ["a", "b", "c"];
    expect(moveItem(original, 2, 0)).toEqual(["c", "a", "b"]);
    expect(original).toEqual(["a", "b", "c"]);
  });

  it("returns the same collection for an invalid move", () => {
    const original = ["a"];
    expect(moveItem(original, 0, 2)).toBe(original);
  });
});
