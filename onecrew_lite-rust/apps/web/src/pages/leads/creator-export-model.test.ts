import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";

import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import type { LeadsT } from "./i18n-labels";
import {
  creatorExportButtonAccessibility,
  creatorExportSuccessCopy,
} from "./creator-export-model";

let t: LeadsT;

beforeAll(async () => {
  const instance = i18next.createInstance();
  await instance.init({
    lng: "zh-CN",
    ns: ["leads"],
    defaultNS: "leads",
    interpolation: { escapeValue: false },
    resources: { "zh-CN": { leads: zhCNLeads } },
  });
  t = instance.getFixedT("zh-CN", "leads");
});

describe("creatorExportSuccessCopy", () => {
  it("names only the safe Desktop destination and file name", () => {
    const copy = creatorExportSuccessCopy({
      destination: "system_desktop",
      format: "xlsx",
      file_name: "reacher-creators-0123456789abcdef.xlsx",
      creator_count: 12,
      content_sha256: `sha256:${"01".repeat(32)}`,
      reused_existing: false,
    }, t);

    expect(copy).toEqual({
      title: "Creator 已导出",
      description: "系统桌面 · reacher-creators-0123456789abcdef.xlsx · 12 位 Creator",
    });
    expect(JSON.stringify(copy)).not.toContain("/Users/");
    expect(JSON.stringify(copy)).not.toContain("C:\\");
  });

  it("distinguishes an idempotently reused file", () => {
    expect(
      creatorExportSuccessCopy({
        destination: "system_desktop",
        format: "xlsx",
        file_name: "reacher-creators-existing.xlsx",
        creator_count: 0,
        content_sha256: `sha256:${"ab".repeat(32)}`,
        reused_existing: true,
      }, t).title,
    ).toBe("Creator 导出文件已存在");
  });

  it("announces the loading state instead of hiding it behind a fixed label", () => {
    expect(creatorExportButtonAccessibility(false, t)).toEqual({
      ariaLabel: "导出 Creator 到系统桌面",
      ariaBusy: false,
    });
    expect(creatorExportButtonAccessibility(true, t)).toEqual({
      ariaLabel: "正在导出 Creator 到系统桌面",
      ariaBusy: true,
    });
  });
});
