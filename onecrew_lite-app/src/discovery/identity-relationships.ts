import type { IdentityRelationship, IdentityReviewReceipt } from "./identity-contracts.ts";

const order = (left: string, right: string): number => Buffer.compare(Buffer.from(left), Buffer.from(right));
export function identityComponents(edges: readonly IdentityRelationship[]): Map<string, string> {
  const adjacency = new Map<string, string[]>();
  for (const [left, right, kind] of edges) if (kind === "same_author") {
    adjacency.set(left, [...(adjacency.get(left) ?? []), right]);
    adjacency.set(right, [...(adjacency.get(right) ?? []), left]);
  }
  const components = new Map<string, string>();
  for (const root of [...adjacency.keys()].sort(order)) {
    const pending = [root];
    while (pending.length) {
      const current = pending.pop()!;
      if (components.has(current)) continue;
      components.set(current, root);
      pending.push(...(adjacency.get(current) ?? []));
    }
  }
  return components;
}
export function validateIdentityAddition(edges: readonly IdentityRelationship[], kind: IdentityReviewReceipt["kind"], ids: readonly [string, string]): void {
  if (kind !== "same_author" && kind !== "different_authors") return;
  for (const [left, right, existing] of edges) if (left === ids[0] && right === ids[1] || left === ids[1] && right === ids[0]) throw new Error(existing === kind ? "identity_review_relationship_already_reviewed" : "identity_review_conflicting_relationship");
  const groups = identityComponents(edges);
  const connected = (left: string, right: string) => (groups.get(left) ?? left) === (groups.get(right) ?? right);
  const conflicts = kind === "same_author" ? edges.some(([left, right, existing]) => existing === "different_authors" && (connected(ids[0], left) && connected(ids[1], right) || connected(ids[0], right) && connected(ids[1], left))) : connected(ids[0], ids[1]);
  if (conflicts) throw new Error("identity_review_conflicting_relationship");
}
