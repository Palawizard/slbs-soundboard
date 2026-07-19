import { describe, expect, it } from "vitest";
import { shortcutFromKeyboardEvent, shortcutLabel } from "./shortcuts";

describe("global shortcut normalization", () => {
  it("uses a stable modifier order and physical letter", () => {
    expect(shortcutFromKeyboardEvent({ code: "KeyQ", ctrlKey: true, altKey: true, shiftKey: true, metaKey: false })).toBe("Ctrl+Alt+Shift+Q");
  });

  it("rejects unmodified, modifier-only, and unknown keys", () => {
    expect(shortcutFromKeyboardEvent({ code: "KeyA", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false })).toBeNull();
    expect(shortcutFromKeyboardEvent({ code: "ControlLeft", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBeNull();
    expect(shortcutFromKeyboardEvent({ code: "NumpadAdd", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBeNull();
  });

  it("renders French modifier names", () => {
    expect(shortcutLabel("Ctrl+Shift+K")).toBe("Ctrl+Maj+K");
    expect(shortcutLabel(null)).toBe("Aucun raccourci");
  });
});
