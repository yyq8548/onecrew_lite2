import { invoke } from "./local-runtime";
import type { CollectEvidenceInput } from "./generated/discovery-contracts";
import { validateEvidenceCollection } from "./generated/discovery-contracts/validators/EvidenceCollection.js";

export async function collectDiscoveryEvidence(input: CollectEvidenceInput) {
  const result = await invoke<unknown>("onecrew_discovery_evidence_collect", { input });
  if (!validateEvidenceCollection(result) || result.campaign_id !== input.campaign_id || result.search_hit_id !== input.search_hit_id)
    throw new Error("discovery_evidence_receipt_invalid");
  return result;
}
