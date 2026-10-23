import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { discoveryIdentityKey, getDiscoveryIdentity } from "@/lib/opencli-discovery";

/** Local receipts only. Focus refresh covers MCP changes; poll only a pending
 * receipt because discovery does not yet publish runtime invalidation events. */
export function SavedIdentityEvidence({ campaignId, hitId }: { campaignId: string; hitId: string }) {
  const { t } = useTranslation("shell");
  const [opened, setOpened] = useState(false);
  const query = useQuery({
    queryKey: discoveryIdentityKey(campaignId, hitId),
    queryFn: () => getDiscoveryIdentity(campaignId, hitId),
    enabled: opened, retry: false, refetchOnWindowFocus: true,
    refetchInterval: state => state.state.data?.status === "reading" && state.state.dataUpdateCount < 12 ? 5000 : false,
  });
  const read = opened ? query.data : undefined;
  return <div className="mt-2 space-y-1 text-xs">
    <Button size="sm" variant="outline" disabled={query.isFetching} onClick={() => {
      if (opened) void query.refetch(); else setOpened(true);
    }}>{t("opencliPilot.savedIdentity")}</Button>
    {opened && query.isError && <p role="status">{t("opencliPilot.savedIdentityMissing")}</p>}
    {read && <>
      <p>{t(`opencliPilot.identityStatus.${read.status}`)}{read.stop_code ? ` · ${read.stop_code}` : ""}</p>
      {read.account_id && <p>{t("opencliPilot.identityAccount", { id: read.account_id })}</p>}
      <a className="text-primary underline" href={read.observed_source_url ?? read.claim.source_url} target="_blank" rel="noreferrer">{t("opencliPilot.identitySource")}</a>
      <p className="text-muted-foreground">{t("opencliPilot.identityLimit")}</p>
    </>}
  </div>;
}
