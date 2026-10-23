import { platformDisplayName } from "@/lib/platform-display";
import { useId, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { authorGroupsKey } from "@/lib/discovery-author-groups";
import { getIdentityReviewMaterial, getIdentityReviewReceipt, identityQuoteOptions, prepareIdentityReview, submitIdentityReview } from "@/lib/discovery-identity-review";
import type { DiscoveryIdentityGroupsSnapshot, SubmitDiscoveryIdentityReviewInput } from "@/lib/generated/discovery-contracts";

type Decision = SubmitDiscoveryIdentityReviewInput["decision"];
type Relationship = SubmitDiscoveryIdentityReviewInput["findings"][number]["relationship"];

export function DiscoveryIdentityReview({ snapshot, available }: { snapshot: DiscoveryIdentityGroupsSnapshot; available: boolean }) {
  const { t } = useTranslation("shell");
  const id = useId();
  const cache = useQueryClient();
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const [decision, setDecision] = useState<Decision>("insufficient");
  const [label, setLabel] = useState("");
  const [reviewerKind, setReviewerKind] = useState<SubmitDiscoveryIdentityReviewInput["reviewer"]["kind"] | "">("");
  const [rationale, setRationale] = useState("");
  const [quotes, setQuotes] = useState<Record<string, string>>({});
  const [relationships, setRelationships] = useState<Record<string, Relationship>>({});
  // Retain the exact request after a failed write. Readback is explicit and
  // never automatically retries or turns a matching ID into write success.
  const [request, setRequest] = useState<SubmitDiscoveryIdentityReviewInput | null>(null);
  const material = useMutation({ mutationFn: getIdentityReviewMaterial });
  const refresh = () => cache.invalidateQueries({ queryKey: authorGroupsKey({
    campaignId: snapshot.campaign_id, revision: snapshot.criteria_revision,
  }) });
  const submit = useMutation({ mutationFn: submitIdentityReview, onSettled: refresh });
  const readback = useMutation({ mutationFn: getIdentityReviewReceipt, onSettled: refresh });
  const sources = snapshot.groups.flatMap(group => group.members.flatMap(member => member.sources.map(source => ({
    id: source.hit.id, text: `${member.display_name} · ${platformDisplayName(member.platform)} · ${source.hit.title}`,
  }))));
  const busy = material.isPending || submit.isPending || readback.isPending;
  const reset = () => {
    setRequest(null); material.reset(); submit.reset(); readback.reset(); setQuotes({}); setRelationships({});
  };
  return <section className="space-y-3 rounded-md border p-3" aria-labelledby={`${id}-title`}>
    <h3 id={`${id}-title`} className="font-medium">{t("opencliPilot.identityReviewTitle")}</h3>
    <p className="text-muted-foreground">{t("opencliPilot.identityReviewHelp")}</p>
    <fieldset disabled={!available || busy || request !== null} className="space-y-3">
      {(["left", "right"] as const).map(side => <div key={side} className="space-y-1">
        <Label htmlFor={`${id}-${side}`}>{t(side === "left" ? "opencliPilot.identityLeft" : "opencliPilot.identityRight")}</Label>
        <Select value={side === "left" ? left : right} onValueChange={value => {
          if (side === "left") setLeft(value); else setRight(value);
          material.reset(); setQuotes({}); setRelationships({});
        }} disabled={!available || busy || request !== null}>
          <SelectTrigger id={`${id}-${side}`} className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>{sources.map(source => <SelectItem key={source.id} value={source.id}>{source.text}</SelectItem>)}</SelectContent>
        </Select>
      </div>)}
      <Button type="button" variant="outline" disabled={!available || !left || !right || left === right || busy || request !== null} onClick={() => {
        material.reset(); setQuotes({}); setRelationships({});
        material.mutate({ campaign_id: snapshot.campaign_id, expected_revision: snapshot.criteria_revision,
          left_search_hit_id: left, right_search_hit_id: right });
      }}>{t("opencliPilot.identityLoad")}</Button>
      {material.isError && <p role="alert">{t("opencliPilot.identityReadError")}</p>}
      {material.data && <>
        {material.data.accounts.map(account => {
          const hit = account.hit.id;
          const options = identityQuoteOptions(account);
          const selected = options.find(option => option.key === quotes[hit]);
          return <div key={hit} className="space-y-2 rounded-md border p-2">
            <p className="font-medium">{account.hit.author_label} · {platformDisplayName(account.hit.platform)} · {account.native_author_id}</p>
            <Label htmlFor={`${id}-${hit}-quote`}>{t("opencliPilot.identityQuote")}</Label>
            <Select value={quotes[hit] ?? "none"} onValueChange={value => setQuotes(current => ({ ...current, [hit]: value }))} disabled={!available || request !== null}>
              <SelectTrigger id={`${id}-${hit}-quote`} className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent><SelectItem value="none">{t("opencliPilot.identityNoQuote")}</SelectItem>{options.map(option =>
                <SelectItem key={option.key} value={option.key}>{option.key} · {option.text.slice(0, 100)}</SelectItem>)}</SelectContent>
            </Select>
            {selected && <blockquote className="max-h-48 overflow-auto whitespace-pre-wrap break-words border-l-2 pl-2">{selected.text}</blockquote>}
            <Label htmlFor={`${id}-${hit}-relation`}>{t("opencliPilot.identityRelationship")}</Label>
            <Select value={relationships[hit] ?? "unclear"} onValueChange={value => setRelationships(current => ({ ...current, [hit]: value as Relationship }))} disabled={!available || request !== null}>
              <SelectTrigger id={`${id}-${hit}-relation`} className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{(["self_owned_account", "other_person", "unclear"] as const).map(value =>
                <SelectItem key={value} value={value}>{t(`opencliPilot.identityRelations.${value}`)}</SelectItem>)}</SelectContent>
            </Select>
          </div>;
        })}
        <Label htmlFor={`${id}-decision`}>{t("opencliPilot.identityDecision")}</Label>
        <Select value={decision} onValueChange={value => setDecision(value as Decision)} disabled={!available || request !== null}>
          <SelectTrigger id={`${id}-decision`} className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>{(["same_author", "different_authors", "insufficient"] as const).map(value =>
            <SelectItem key={value} value={value}>{t(`opencliPilot.identityDecisions.${value}`)}</SelectItem>)}</SelectContent>
        </Select>
        <Label htmlFor={`${id}-reviewer-kind`}>{t("opencliPilot.reviewReviewerKind")}</Label>
        <Select value={reviewerKind || "none"} onValueChange={value => setReviewerKind(value === "none" ? "" : value as SubmitDiscoveryIdentityReviewInput["reviewer"]["kind"])} disabled={!available || request !== null}>
          <SelectTrigger id={`${id}-reviewer-kind`} className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="none">{t("opencliPilot.reviewReviewerKindSelect")}</SelectItem><SelectItem value="human">{t("opencliPilot.reviewReviewerHuman")}</SelectItem><SelectItem value="agent">{t("opencliPilot.reviewReviewerAgent")}</SelectItem></SelectContent>
        </Select>
        <Label htmlFor={`${id}-label`}>{t("opencliPilot.identityReviewer")}</Label>
        <Input id={`${id}-label`} value={label} onChange={event => setLabel(event.target.value)} maxLength={200} />
        <Label htmlFor={`${id}-rationale`}>{t("opencliPilot.identityRationale")}</Label>
        <Textarea id={`${id}-rationale`} value={rationale} onChange={event => setRationale(event.target.value)} maxLength={4000} />
        <Button type="button" disabled={!available || busy || request !== null || !reviewerKind || !label.trim() || !rationale.trim()} onClick={() => {
          if (!material.data || !reviewerKind) return;
          const input = prepareIdentityReview(material.data, {
            decisionId: crypto.randomUUID(), decision, label, rationale, quotes, relationships, reviewerKind,
          });
          setRequest(input); submit.mutate(input);
        }}>{t("opencliPilot.identitySubmit")}</Button>
      </>}
    </fieldset>
    {busy && <p role="status">{t("opencliPilot.identityWorking")}</p>}
    {request && <p className="break-all">{t("opencliPilot.identityRequest", { id: request.decision_id })}</p>}
    {submit.isSuccess && <p role="status">{t("opencliPilot.identitySaved")}</p>}
    {submit.isError && <p role="alert">{t("opencliPilot.identityUnknown")}</p>}
    {request && <Button variant="outline" disabled={busy} onClick={() => readback.mutate({ campaign_id: request.pair.campaign_id, decision_id: request.decision_id })}>{t("opencliPilot.identityReadback")}</Button>}
    {readback.isError && <p role="alert">{t("opencliPilot.identityReadError")}</p>}
    {readback.isSuccess && <p role="status">{t(readback.data ? "opencliPilot.identityReceiptFound" : "opencliPilot.identityReceiptMissing")}</p>}
    {readback.data && <dl className="break-all">
      <dt>{t("opencliPilot.identityDecision")}</dt><dd>{readback.data.kind}</dd>
      <dt>{t("opencliPilot.identityReviewer")}</dt><dd>{readback.data.operator_account_id}</dd>
      <dt>{t("opencliPilot.identityAccounts")}</dt><dd>{readback.data.platforms.map((platform, index) => `${platformDisplayName(platform)}:${readback.data?.native_author_ids[index]}`).join(" · ")}</dd>
    </dl>}
    {submit.isError && readback.isSuccess && request && <Button variant="outline" disabled={busy} onClick={() => submit.mutate(request)}>{t("opencliPilot.identityReplay")}</Button>}
    {(submit.isSuccess || readback.data) && <Button variant="outline" disabled={busy} onClick={reset}>{t("opencliPilot.identityNext")}</Button>}
  </section>;
}
