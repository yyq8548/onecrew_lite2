export function pendingEvidenceReasons(
  reasons: readonly string[] | undefined, rated: boolean, scoringFailed: boolean,
): string[] {
  const result = [...new Set(reasons ?? [])];
  if (scoringFailed) result.push("score_temporarily_failed");
  else if (!rated && !result.includes("initial_match_unrated")) result.push("initial_match_unrated");
  return [...new Set(result)];
}
