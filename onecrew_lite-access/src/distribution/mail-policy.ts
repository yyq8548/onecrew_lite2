export type DistributionMailDecision = {
  application_id: string;
  kind: "approved" | "rejected";
  decision_revision: number;
  recipient: string;
};
// Decision revisions are stable across a mail retry, but change on review or resubmission.
// Rejection notifications are eligible without a download entitlement.
export async function distributionMailEligible(
  db: D1Database,
  event: DistributionMailDecision,
) {
  const current = await db
    .prepare(
      `SELECT 1 AS eligible FROM distribution_application a
 JOIN "user" u ON u.id=a.user_id LEFT JOIN distribution_entitlement e ON e.user_id=a.user_id
 WHERE a.id=? AND a.status=? AND a.decision_revision=? AND u.email=? AND u."emailVerified"=1
 AND (?='rejected' OR e.status='active')`,
    )
    .bind(
      event.application_id,
      event.kind,
      event.decision_revision,
      event.recipient,
      event.kind,
    )
    .first();
  return current !== null;
}
