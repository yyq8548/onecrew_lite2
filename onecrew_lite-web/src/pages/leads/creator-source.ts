import {
  creatorAccountViews,
  type CreatorAccountView,
  type CreatorRecord,
} from "@/lib/creator";

export type CreatorSourcePresentation =
  | { kind: "platform"; accounts: CreatorAccountView[] }
  | { kind: "manual_entry" }
  | { kind: "creator_import" }
  | { kind: "none" };

export function creatorSourcePresentation(record: CreatorRecord): CreatorSourcePresentation {
  const accounts = creatorAccountViews(record);
  if (accounts.length) return { kind: "platform", accounts };
  if (record.origins.some((origin) => origin.kind === "manual_entry")) {
    return { kind: "manual_entry" };
  }
  if (record.origins.some((origin) => origin.kind === "creator_import")) {
    return { kind: "creator_import" };
  }
  return { kind: "none" };
}
