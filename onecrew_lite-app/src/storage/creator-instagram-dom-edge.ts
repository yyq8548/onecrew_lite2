import type { DatabaseSync } from "node:sqlite";
import { rebuildCreatorTableChanges } from "./creator-schema.ts";

export function migrateCreatorInstagramDomEdge(db: DatabaseSync): void {
  rebuildCreatorTableChanges(db, "discovery_web_source_reads", [[
    "json_extract(native_material_json,'$.identity.execution_mode') IS 'computer_use'",
    "json_extract(native_material_json,'$.identity.execution_mode') IN ('computer_use','opencli_dom')",
  ]]);
}
