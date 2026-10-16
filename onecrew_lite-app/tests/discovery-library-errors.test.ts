import assert from "node:assert/strict";
import test from "node:test";
import { TableDataError } from "onecrew_lite_tables/embedded";
import { LibraryProjectionError, mapLibraryProjectionError } from "../src/sync/discovery-library.ts";

test("Library 原始结构化失败保留可重准备、冲突和未知失败边界", () => {
  for (const [code, expected] of [
    ["data.projection_stale", "library_source_stale"],
    ["data.identity_stale", "library_source_stale"],
    ["data.identity_version_conflict", "library_projection_conflict"],
    ["data.projection_row_conflict", "library_projection_conflict"],
    ["data.projection_row_missing", "library_row_missing"],
    ["data.projection_unmanaged_table", "library_unmanaged_table"],
  ]) {
    const original = new TableDataError(code!, 409, code);
    assert.throws(() => mapLibraryProjectionError(original, true), error =>
      error instanceof LibraryProjectionError && error.code === expected && error.cause === original);
  }
  const unexpected = new TypeError("unexpected implementation failure");
  assert.throws(() => mapLibraryProjectionError(unexpected, true), error => error === unexpected);
  const unknownWrite = new TableDataError("interrupted write", 500, "engine.error");
  assert.throws(() => mapLibraryProjectionError(unknownWrite, true), error =>
    error instanceof LibraryProjectionError && error.code === "library_write_failed" && error.cause === unknownWrite);
});
