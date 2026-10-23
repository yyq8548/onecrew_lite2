import { describe, expect, it } from "vitest";

import {
  ComposerFormatToolbar,
  FONT_SIZE_DEFAULT_VALUE,
  FORMAT_TOOLBAR_COMMAND_KEYS,
  fontFamilyFromMenuValue,
  fontSizeFromMenuValue,
  fontSizeMenuValue,
  fontSizeTriggerLabel,
  type FormatToolbarCommand,
} from "@/components/mail/composer-format-toolbar";
import { EMAIL_FONT_OPTIONS, EMAIL_FONT_SIZES } from "@/components/mail/markdown-email";

describe("ComposerFormatToolbar", () => {
  it("exports the toolbar component", () => {
    expect(typeof ComposerFormatToolbar).toBe("function");
  });

  it("maps every toolbar command to a mail translation key", () => {
    const commands = Object.keys(FORMAT_TOOLBAR_COMMAND_KEYS) as FormatToolbarCommand[];
    for (const command of commands) {
      expect(FORMAT_TOOLBAR_COMMAND_KEYS[command]).toBe(`toolbar.commands.${command}`);
    }
    expect(commands).toHaveLength(12);
  });
});

describe("font size menu value mapping", () => {
  it("round-trips null as the default sentinel", () => {
    expect(fontSizeMenuValue(null)).toBe(FONT_SIZE_DEFAULT_VALUE);
    expect(fontSizeFromMenuValue(FONT_SIZE_DEFAULT_VALUE)).toBeNull();
    expect(fontSizeTriggerLabel(null, "默认")).toBe("默认");
  });

  it("round-trips every listed size", () => {
    for (const size of EMAIL_FONT_SIZES) {
      expect(fontSizeFromMenuValue(fontSizeMenuValue(size))).toBe(size);
      expect(fontSizeTriggerLabel(size, "默认")).toBe(String(size));
    }
  });

  it("treats garbage as the default size", () => {
    expect(fontSizeFromMenuValue("")).toBeNull();
    expect(fontSizeFromMenuValue("abc")).toBeNull();
    expect(fontSizeFromMenuValue("-3")).toBeNull();
  });
});

describe("font family menu value mapping", () => {
  it("maps every option value to its family", () => {
    for (const option of EMAIL_FONT_OPTIONS) {
      expect(fontFamilyFromMenuValue(option.value)).toBe(option.family);
    }
  });

  it("falls back to the system default for unknown values", () => {
    expect(fontFamilyFromMenuValue("nope")).toBeNull();
  });
});
