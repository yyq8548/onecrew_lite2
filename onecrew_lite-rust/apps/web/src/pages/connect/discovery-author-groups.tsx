import { platformDisplayName } from "@/lib/platform-display";
import { useState } from "react";
import type { UseQueryResult } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { authorProfileLink } from "@/lib/discovery-author-groups";
import type { DiscoveryIdentityGroupsSnapshot } from "@/lib/generated/discovery-contracts";
import { DiscoveryIdentityReview } from "./discovery-identity-review";
import { DiscoveryIdentityRevoke } from "./discovery-identity-revoke";

export function DiscoveryAuthorGroups({ groups }: { groups: UseQueryResult<DiscoveryIdentityGroupsSnapshot, Error> }) {
  const { t, i18n } = useTranslation("shell");
  const [open, setOpen] = useState(false);
  const campaignId = groups.data?.campaign_id;
  const revision = groups.data?.criteria_revision;
  const format = (value: number) => new Intl.NumberFormat(i18n.resolvedLanguage).format(value);
  return <Collapsible open={open} onOpenChange={setOpen} className="discovery-disclosure">
    <CollapsibleTrigger asChild><Button variant="ghost">{t("opencliPilot.authorGroupsTitle")}</Button></CollapsibleTrigger>
    <CollapsibleContent forceMount hidden={!open} className="space-y-3 pt-3">
      <p className="text-muted-foreground">{t("opencliPilot.authorGroupsHelp")}</p>
      <Button variant="outline" disabled={groups.isFetching} onClick={() => { void groups.refetch(); }}>{t("opencliPilot.authorGroupsRefresh")}</Button>
      {groups.isPending && <p role="status">{t("opencliPilot.authorGroupsLoading")}</p>}
      {groups.isError && <Alert variant="destructive"><AlertDescription>{t("opencliPilot.authorGroupsError")}</AlertDescription></Alert>}
      {groups.data && <DiscoveryIdentityReview key={`${campaignId}:${revision}`} snapshot={groups.data} available={!groups.isError && !groups.isFetching} />}
      {groups.data && <DiscoveryIdentityRevoke key={`revoke:${campaignId}:${revision}`} snapshot={groups.data} available={!groups.isError && !groups.isFetching} />}
      {!groups.isError && groups.data && <>
        <p>{t("opencliPilot.authorGroupsCounts", {
          accounts: format(groups.data.counts.native_accounts), groups: format(groups.data.counts.identity_groups),
          qualified: format(groups.data.counts.qualified_identity_groups), nativeOnly: format(groups.data.counts.qualified_native_only_groups),
        })}</p>
        <p className="text-muted-foreground">{t("opencliPilot.authorGroupsGaps", {
          unbound: format(groups.data.counts.unbound_source_hits), stale: format(groups.data.counts.stale_identity_reviews),
        })}</p>
        {groups.data.groups.length === 0 && <p>{t("opencliPilot.authorGroupsEmpty")}</p>}
        <ul className="space-y-2">{groups.data.groups.map(group => <li key={group.group_id} className="rounded-md border p-3 space-y-2">
          <p className="font-medium">{t(group.has_same_author_review ? "opencliPilot.authorGroupReviewed" : "opencliPilot.authorGroupNativeOnly")}</p>
          <ul>{group.members.map(member => {
            const link = authorProfileLink(member.profile_url);
            const verdict = member.verdict === "qualified" || member.verdict === "pending" || member.verdict === "rejected"
              ? t(`opencliPilot.verdict.${member.verdict}`) : member.verdict;
            return <li key={member.candidate_id} className="break-words">
              {link ? <a href={link} target="_blank" rel="noreferrer" className="text-primary underline">{member.display_name}</a> : member.display_name}
              {" · "}{platformDisplayName(member.platform)}{" · "}{verdict}
            </li>;
          })}</ul>
        </li>)}</ul>
      </>}
    </CollapsibleContent>
  </Collapsible>;
}
