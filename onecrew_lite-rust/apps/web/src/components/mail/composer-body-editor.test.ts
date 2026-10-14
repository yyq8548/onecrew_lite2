import { describe, expect, it } from "vitest";

import {
  ADDRESS_INPUT_CLASS,
  AddressRow,
  ComposerBodyEditor,
} from "@/components/mail/composer-body-editor";

describe("ComposerBodyEditor", () => {
  it("exports the shared body editor (a forwardRef exotic component) and address row", () => {
    // forwardRef 返回的是带 render 的对象而不是普通函数：模板编辑器通过 ref 调 insertText。
    expect(typeof ComposerBodyEditor).toBe("object");
    expect(typeof (ComposerBodyEditor as unknown as { render: unknown }).render).toBe("function");
    expect(typeof AddressRow).toBe("function");
  });

  it("keeps the address input borderless so the row's focus-within ground is the focus state", () => {
    expect(ADDRESS_INPUT_CLASS).toContain("border-0");
    expect(ADDRESS_INPUT_CLASS).toContain("focus-visible:ring-0");
    expect(ADDRESS_INPUT_CLASS).not.toMatch(/#[0-9a-f]{3,6}/i);
    expect(ADDRESS_INPUT_CLASS).not.toContain("bg-white");
  });
});
