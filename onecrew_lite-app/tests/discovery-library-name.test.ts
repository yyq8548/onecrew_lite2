import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { discoveryProductName } from "../src/discovery/library-name.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";

test("原Library名称只取产品和批准人数，保留历史自动名称与同日创建序号", () => {
  for (const [brief, expected] of [["我要卖键盘，找键盘测评或数码推荐达人。粉丝至少 8000，目标 2 名推荐作者", "键盘"], ["手机支架\nContent language: 英文", "手机支架"], ["我卖蓝牙音响，粉丝10000以上", "蓝牙音响"], ["我要卖机械键盘 想找测评达人", "机械键盘"], ["I sell mechanical keyboards, find reviewers", "mechanical keyboards"]]) assert.equal(discoveryProductName(brief!), expected);
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-profile-hydration/after.sqlite", import.meta.url)), { readOnly: true });
  try {
    const name = new DiscoveryRepository(db).getLibraryTableName("profile-hydration");
    assert.equal(name.name, "Keyboard creators2个"); assert.match(name.previous_auto_name, /^Keyboard creators_[0-9]{4}_1$/u);
  } finally { db.close(); }
});
