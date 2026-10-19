import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { getDiscoveryCampaign, increaseDiscoveryTarget, type DiscoveryCampaign, type DiscoveryTargetIncrease } from "@/lib/opencli-discovery";

export function DiscoveryTargetIncrease({ campaign, busy }: { campaign: DiscoveryCampaign; busy: boolean }) {
  const { t } = useTranslation("shell");
  const client = useQueryClient();
  const [target, setTarget] = useState(0);
  const [request, setRequest] = useState<DiscoveryTargetIncrease | null>(null);
  const [readbackFailed, setReadbackFailed] = useState(false);
  const increase = useMutation({
    mutationFn: increaseDiscoveryTarget,
    retry: false,
    onSuccess: result => {
      client.setQueryData(["opencli-campaign", result.id], result);
      setRequest(null);
      setTarget(0);
      setReadbackFailed(false);
    },
    onError: async (_error, input) => {
      try {
        await client.fetchQuery({ queryKey: ["opencli-campaign", input.campaign_id],
          queryFn: () => getDiscoveryCampaign(input.campaign_id), staleTime: 0, retry: false });
        setReadbackFailed(false);
      } catch { setReadbackFailed(true); }
    },
  });
  const valid = Number.isInteger(target) && target >= 1 && target !== campaign.target_count && target <= 10_000;
  const submit = () => {
    if (busy || increase.isPending || (!request && !valid)) return;
    // Keep the full original request after an uncertain response, even if a
    // subsequent Campaign read shows the new target. Replay uses the same ID.
    const input = request ?? { campaign_id: campaign.id, expected_revision: campaign.revision,
      expected_target_count: campaign.target_count, new_target_count: target, decision_id: crypto.randomUUID() };
    setRequest(input);
    increase.mutate(input);
  };
  return <div className="discovery-adjustment space-y-2">
    <p className="font-medium">{t("opencliPilot.targetDecisionTitle")}</p>
    <p>{t("opencliPilot.targetDecisionCurrent", { target: campaign.target_count })}</p>
    <p className="text-sm text-muted-foreground">{t("opencliPilot.targetDecisionScope", { used: campaign.used_calls, max: campaign.max_calls ?? "∞" })}</p>
    <div className="flex flex-wrap items-end gap-2">
      <div className="grid gap-1"><Label htmlFor="discovery-target-new">{t("opencliPilot.targetDecisionNew")}</Label>
        <Input id="discovery-target-new" className="w-36" type="number" min={1} max={10000}
          value={target || ""} disabled={increase.isPending || readbackFailed}
          onChange={event => { setTarget(Number(event.target.value)); setRequest(null); }} /></div>
      <Button variant="outline" disabled={busy || increase.isPending || (!request && !valid)} onClick={submit}>
        {increase.isPending && <Loader2 className="size-4 animate-spin" />}
        {request || target ? t("opencliPilot.targetDecisionConfirm", { target: request?.new_target_count ?? target }) : t("opencliPilot.confirmChange")}
      </Button>
    </div>
    {increase.isError && <Alert><AlertDescription>{t("opencliPilot.targetDecisionUncertain")}</AlertDescription></Alert>}
  </div>;
}
