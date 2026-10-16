import { describe, expect, it } from "vitest";

import manifest from "@mcp-ui/lib/generated/contracts/manifest.json";
import { RESULT_VIEW_KINDS } from "@mcp-ui/features/result-card/components/result-views";
import { RETIRED_PRESENTATIONS } from "@mcp-ui/features/result-card/retired-sending";
import { CONTINUATION_CONTEXT_KINDS, PENDING_ACTION_KINDS, parseResultSnapshot } from "@mcp-ui/lib/result-contract";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

// Closed vocabularies of the generated manifest: every string `enum` and every
// discriminated union whose branches carry a `kind` const.
function manifestVocabularies(): string[][] {
  const vocabularies: string[][] = [];
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (typeof value !== "object" || value === null) return;
    const node = value as Record<string, unknown>;
    if (Array.isArray(node.enum) && node.enum.every((member) => typeof member === "string")) {
      vocabularies.push(node.enum as string[]);
    }
    for (const union of [node.oneOf, node.anyOf]) {
      if (!Array.isArray(union) || union.length === 0) continue;
      const kinds = union.map((branch) => {
        const properties = (branch as { properties?: Record<string, { const?: unknown }> } | null)?.properties;
        return properties?.kind?.const;
      });
      if (kinds.every((kind) => typeof kind === "string")) vocabularies.push(kinds as string[]);
    }
    Object.values(node).forEach(visit);
  };
  visit(manifest);
  return vocabularies;
}

// The one manifest vocabulary that contains a distinctive member.
function vocabularyContaining(member: string): string[] {
  const distinct = new Map<string, string[]>();
  for (const vocabulary of manifestVocabularies()) {
    if (vocabulary.includes(member)) distinct.set(JSON.stringify([...vocabulary].sort()), vocabulary);
  }
  expect([...distinct.values()], `manifest vocabularies containing ${member}`).toHaveLength(1);
  return [...distinct.values()][0];
}

function expectSameMembers(label: string, handwritten: readonly string[], generated: readonly string[]): void {
  expect(new Set(handwritten).size, `${label}: handwritten kinds repeat a member`).toBe(handwritten.length);
  const drift = {
    missingFromHandwritten: generated.filter((member) => !handwritten.includes(member)).sort(),
    extraInHandwritten: handwritten.filter((member) => !generated.includes(member)).sort(),
  };
  expect(drift, `${label}: handwritten kinds differ from generated manifest.json`).toEqual({
    missingFromHandwritten: [],
    extraInHandwritten: [],
  });
}

describe("handwritten kind vocabularies match the generated contract manifest", () => {
  it("gives every presentation kind a business view (result-views.tsx)", () => {
    expectSameMembers("presentation kinds", [...RESULT_VIEW_KINDS, ...RETIRED_PRESENTATIONS], vocabularyContaining("search_plan_editor"));
  });

  it("checks the semantics of every pending-action kind (parse.ts)", () => {
    expectSameMembers("pending-action kinds", PENDING_ACTION_KINDS, vocabularyContaining("search_plan"));
  });

  it("parses every continuation-context kind (parse.ts)", () => {
    expectSameMembers("continuation-context kinds", CONTINUATION_CONTEXT_KINDS, vocabularyContaining("search_plan_confirmed"));
  });

  // parse.ts and the views treat resource kinds as opaque, so the handwritten
  // layer must accept every generated kind rather than enumerate a subset.
  it("accepts every resource kind without a handwritten subset (parse.ts)", () => {
    const resourceKinds = vocabularyContaining("provider_object");
    const rejected = resourceKinds.filter((kind) => parseResultSnapshot({
      ...SNAPSHOT,
      batch: { outcomes: SNAPSHOT.batch.outcomes.map((outcome) => ({
        ...outcome,
        resource_refs: [{ kind, id: "resource-1", label: "Resource" }],
      })) },
    }) === null);
    expect(rejected, "resource kinds rejected by the handwritten layer").toEqual([]);
    expect(parseResultSnapshot({
      ...SNAPSHOT,
      batch: { outcomes: SNAPSHOT.batch.outcomes.map((outcome) => ({
        ...outcome,
        resource_refs: [{ kind: "retired_kind", id: "resource-1", label: "Resource" }],
      })) },
    })).toBeNull();
  });
});
