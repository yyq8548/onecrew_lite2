import { useId, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { authorGroupsKey } from "@/lib/discovery-author-groups";
import { platformDisplayName } from "@/lib/platform-display";
import { getIdentityReviewReceipt, revokeIdentityReview } from "@/lib/discovery-identity-review";
import type { DiscoveryIdentityGroupsSnapshot, RevokeDiscoveryIdentityReviewInput } from "@/lib/generated/discovery-contracts";

export function DiscoveryIdentityRevoke({ snapshot, available }: { snapshot: DiscoveryIdentityGroupsSnapshot; available: boolean }) {
  const { t } = useTranslation("shell");
  const id = useId();
  const cache = useQueryClient();
  const [target, setTarget] = useState("");
  const [label, setLabel] = useState("");
  const [reason, setReason] = useState("");
  const [request, setRequest] = useState<RevokeDiscoveryIdentityReviewInput | null>(null);
  const refresh = () => cache.invalidateQueries({ queryKey: authorGroupsKey({ campaignId: snapshot.campaign_id, revision: snapshot.criteria_revision }) });
  const revoke = useMutation({ mutationFn: revokeIdentityReview, onSettled: refresh });
  const readback = useMutation({ mutationFn: getIdentityReviewReceipt, onSettled: refresh });
  const busy = revoke.isPending || readback.isPending;
  // The owner authorizes the operator and validates the target atomically. Stale
  // evidence must not prevent withdrawing an earlier incorrect review.
  const reviews = snapshot.reviews.filter(review => review.receipt.kind !== "revoke" && review.state !== "revoked");
  const selected = reviews.find(review => review.receipt.decision_id === target);
  return <section className="space-y-3 rounded-md border p-3" aria-labelledby={`${id}-title`}>
    <h3 id={`${id}-title`} className="font-medium">{t("opencliPilot.identityRevokeTitle")}</h3>
    <p className="text-muted-foreground">{t("opencliPilot.identityRevokeHelp")}</p>
    {!reviews.length && !request && <p>{t("opencliPilot.identityRevokeEmpty")}</p>}
    <fieldset disabled={!available || busy || request !== null} className="space-y-2">
      <Label htmlFor={`${id}-target`}>{t("opencliPilot.identityRevokeTarget")}</Label>
      <Select value={target} onValueChange={setTarget} disabled={!available || busy || request !== null}>
        <SelectTrigger id={`${id}-target`} className="w-full"><SelectValue /></SelectTrigger>
        <SelectContent>{reviews.map(({ receipt }) => <SelectItem key={receipt.decision_id} value={receipt.decision_id}>
          {receipt.decision_id} · {receipt.platforms.map((platform, index) => `${platformDisplayName(platform)}:${receipt.native_author_ids[index]}`).join(" / ")}
        </SelectItem>)}</SelectContent>
      </Select>
      {selected && <p className="break-all">{t("opencliPilot.identityDecision")}: {selected.receipt.kind} · {selected.state}</p>}
      <Label htmlFor={`${id}-reviewer`}>{t("opencliPilot.identityReviewer")}</Label>
      <Input id={`${id}-reviewer`} value={label} maxLength={200} onChange={event => setLabel(event.target.value)} />
      <Label htmlFor={`${id}-reason`}>{t("opencliPilot.identityRationale")}</Label>
      <Textarea id={`${id}-reason`} value={reason} maxLength={4000} onChange={event => setReason(event.target.value)} />
      <Button variant="outline" disabled={!available || busy || !!request || !selected || !label.trim() || !reason.trim()} onClick={() => {
        if (!selected) return;
        const input: RevokeDiscoveryIdentityReviewInput = { campaign_id: snapshot.campaign_id, expected_revision: snapshot.criteria_revision,
          decision_id: crypto.randomUUID(), revokes_decision_id: selected.receipt.decision_id,
          reviewer: { kind: "human", label: label.trim() }, rationale: reason.trim() };
        setRequest(input); revoke.mutate(input);
      }}>{t("opencliPilot.identityRevokeConfirm")}</Button>
    </fieldset>
    {busy && <p role="status">{t("opencliPilot.identityWorking")}</p>}
    {request && <p className="break-all">{t("opencliPilot.identityRequest", { id: request.decision_id })}</p>}
    {revoke.isSuccess && <p role="status">{t("opencliPilot.identityRevokeSaved")}</p>}
    {revoke.isError && <p role="alert">{t("opencliPilot.identityUnknown")}</p>}
    {request && <Button variant="outline" disabled={busy} onClick={() => readback.mutate({ campaign_id: request.campaign_id, decision_id: request.decision_id })}>{t("opencliPilot.identityReadback")}</Button>}
    {readback.isError && <p role="alert">{t("opencliPilot.identityReadError")}</p>}
    {readback.isSuccess && <p role="status">{t(readback.data ? "opencliPilot.identityReceiptFound" : "opencliPilot.identityReceiptMissing")}</p>}
    {readback.data && <p className="break-all">{readback.data.kind} · {readback.data.revokes_decision_id} · {readback.data.operator_account_id}</p>}
    {revoke.isError && readback.isSuccess && request && <Button variant="outline" disabled={busy} onClick={() => revoke.mutate(request)}>{t("opencliPilot.identityRevokeReplay")}</Button>}
    {(revoke.isSuccess || readback.data) && <Button variant="outline" disabled={busy} onClick={() => {
      setRequest(null); setTarget(""); setReason(""); revoke.reset(); readback.reset();
    }}>{t("opencliPilot.identityRevokeNext")}</Button>}
  </section>;
}
