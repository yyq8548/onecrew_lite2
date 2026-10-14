import { useState } from "react";
import { FolderInput, Loader2, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  CREATOR_GROUPS_QUERY_KEY,
  bulkMoveCreatorsToGroup,
} from "@/lib/creator-groups";
import {
  CREATORS_QUERY_KEY,
  type CreatorGroupDestination,
  type CreatorGroupSummary,
} from "@/lib/creator";

const UNCATEGORIZED_VALUE = "__uncategorized__";

export function CreatorBulkMoveBar({
  creatorIds,
  groups,
  groupsLoading,
  groupsError,
  onRetryGroups,
  onClear,
}: {
  creatorIds: string[];
  groups: CreatorGroupSummary[];
  groupsLoading: boolean;
  groupsError: string | null;
  onRetryGroups: () => void;
  onClear: () => void;
}) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [destination, setDestination] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const overLimit = creatorIds.length > 100;

  if (creatorIds.length === 0) return null;

  const move = async () => {
    if (!destination || overLimit) return;
    const target: CreatorGroupDestination = destination === UNCATEGORIZED_VALUE
      ? { kind: "uncategorized" }
      : { kind: "group", group_id: destination };
    setPending(true);
    try {
      const result = await bulkMoveCreatorsToGroup({
        creator_ids: creatorIds,
        destination: target,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: CREATOR_GROUPS_QUERY_KEY }),
      ]);
      toast.success(t("bulkMove.success", { count: result.moved_count }), {
        description: result.unchanged_count > 0
          ? t("bulkMove.unchanged", { count: result.unchanged_count })
          : undefined,
      });
      setDestination(null);
      onClear();
    } catch (cause) {
      toast.error(t("bulkMove.failed"), { description: String(cause) });
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-md border bg-muted/40 px-3 py-2">
      <span className="text-sm font-medium">{t("bulkMove.selected", { count: creatorIds.length })}</span>
      <Select
        value={destination ?? undefined}
        onValueChange={setDestination}
        disabled={pending || groupsLoading || Boolean(groupsError)}
      >
        <SelectTrigger size="sm" className="min-w-44 bg-background">
          <SelectValue placeholder={groupsLoading ? t("bulkMove.loadingGroups") : t("bulkMove.selectDestination")} />
        </SelectTrigger>
        <SelectContent viewportClassName="max-h-72">
          <SelectItem value={UNCATEGORIZED_VALUE}>{t("bulkMove.uncategorized")}</SelectItem>
          {groups.map(({ group }) => (
            <SelectItem key={group.id} value={group.id}>{group.name}</SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        size="sm"
        disabled={!destination || pending || overLimit || Boolean(groupsError)}
        onClick={() => void move()}
      >
        {pending ? <Loader2 className="size-4 animate-spin" /> : <FolderInput className="size-4" />}
        {pending ? t("bulkMove.submitting") : t("bulkMove.submit")}
      </Button>
      <Button variant="ghost" size="sm" disabled={pending} onClick={onClear}>
        <X className="size-4" />{t("bulkMove.clear")}
      </Button>
      {groupsError && (
        <div className="flex items-center gap-2" role="alert">
          <span className="text-xs text-destructive">{t("bulkMove.incompleteGroups")}</span>
          <Button variant="outline" size="sm" disabled={pending} onClick={onRetryGroups}>{t("common.actions.retry")}</Button>
        </div>
      )}
      {overLimit && <span role="alert" className="text-xs text-destructive">{t("bulkMove.overLimit")}</span>}
    </div>
  );
}
