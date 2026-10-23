import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { compileNativePlatformSearch, nativePlatformSearchCatalog } from "../src/discovery/native-search-contracts.ts";
import { searchPlanScope } from "../src/discovery/search-plan-contracts.ts";

test("五个原生搜索的原生产registry schema/revision及typed默认scope逐项符合Rust oracle", () => {
  const expected = z.array(z.object({ operation_id: z.string(), connection: z.unknown(), input_schema: z.unknown(), schema_revision: z.string(), initial_scope: searchPlanScope, initial_token: z.string() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-native-search-rust.json", import.meta.url))));
  assert.equal(expected.length, 5); const catalog = nativePlatformSearchCatalog();
  for (const row of expected) {
    const actual = catalog.find(item => item.operation_id === row.operation_id);
    assert.deepEqual(actual, { operation_id: row.operation_id, connection: row.connection, input_schema: row.input_schema, schema_revision: row.schema_revision });
    const initial = compileNativePlatformSearch(row.initial_scope, "initial");
    assert.equal(initial.token, row.initial_token); assert.equal(typeof initial.operationType, "symbol");
    assert.throws(() => compileNativePlatformSearch({ ...row.initial_scope, schema_revision: "wrong" }, "initial"));
    assert.throws(() => compileNativePlatformSearch({ ...row.initial_scope, arguments: { ...row.initial_scope.arguments, search_plan_policy: "user_requested_direct" } }, "initial"));
    assert.throws(() => compileNativePlatformSearch({ ...row.initial_scope, arguments: { ...row.initial_scope.arguments, confirmed_plan_ref: "client-approval" } }, "initial"));
    if (row.operation_id.startsWith("youtube.") || row.operation_id.startsWith("x.")) {
      const next = { ...row.initial_scope, arguments: { ...row.initial_scope.arguments, cursor: "native-page", confirmed_plan_ref: "confirmed" } };
      assert.throws(() => compileNativePlatformSearch(next, "initial"));
      const compiled = compileNativePlatformSearch(next, "execute"); assert.equal(compiled.token, initial.token); assert.equal(compiled.operationType, initial.operationType); assert.notDeepEqual(compiled.request, initial.request);
    }
  }
});
