import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  discoveryPublicPageKey, getDiscoveryPublicPage, publicPageRefreshInterval,
  type DiscoveryPublicPageRead,
} from "@/lib/opencli-discovery";

type Props = { campaignId: string; candidateId: string; requestedUrl: string };

/** Receipt text is untrusted source data, not executable Markdown or a verdict. */
export function PublicPageReceipt({ read }: { read: DiscoveryPublicPageRead }) {
  const { t } = useTranslation("shell");
  return <div className="space-y-1 break-words text-xs">
    <p role="status">{t(`opencliPilot.publicPageStatus.${read.status}`)}{read.stop_code ? ` · ${read.stop_code}` : ""}</p>
    <p>{t("opencliPilot.publicPageSource")}: {read.source_url}</p>
    <p className="whitespace-pre-wrap">{read.source_excerpt}</p>
    <p>{t("opencliPilot.publicPageRequested")}: {read.requested_url}</p>
    {read.status === "completed" && <>
      <p>{t("opencliPilot.publicPageDocument")}: {read.document_url}</p>
      <p className="font-medium">{read.title}</p>
      <p className="max-h-64 overflow-y-auto whitespace-pre-wrap">{read.markdown_excerpt}</p>
      {read.truncated && <p>{t("opencliPilot.publicPageTruncated")}</p>}
    </>}
    <p className="text-muted-foreground">{t("opencliPilot.publicPageLimit")}</p>
  </div>;
}

export function SavedPublicPageEvidence(props: Props) {
  // A different Campaign/candidate/URL starts closed, with no prior receipt.
  return <PublicPageEvidence key={JSON.stringify(discoveryPublicPageKey(props.campaignId, props.candidateId, props.requestedUrl))} {...props} />;
}

function PublicPageEvidence({ campaignId, candidateId, requestedUrl }: Props) {
  const { t } = useTranslation("shell");
  const [opened, setOpened] = useState(false);
  const query = useQuery({
    queryKey: discoveryPublicPageKey(campaignId, candidateId, requestedUrl),
    queryFn: () => getDiscoveryPublicPage(campaignId, candidateId, requestedUrl),
    enabled: opened, retry: false, refetchOnWindowFocus: true,
    refetchInterval: state => publicPageRefreshInterval(state.state.data?.status, state.state.dataUpdateCount, state.state.status === "error"),
  });
  return <div className="mt-2 space-y-2">
    <Button size="sm" variant="outline" disabled={query.isFetching} onClick={() => {
      if (opened) void query.refetch(); else setOpened(true);
    }}>{t("opencliPilot.savedPublicPage")}</Button>
    {opened && query.isFetching && <p role="status">{t("opencliPilot.publicPageLoading")}</p>}
    {opened && query.isError && <p role="status">{t("opencliPilot.publicPageUnavailable")}</p>}
    {opened && !query.isError && query.data && <PublicPageReceipt read={query.data} />}
  </div>;
}
