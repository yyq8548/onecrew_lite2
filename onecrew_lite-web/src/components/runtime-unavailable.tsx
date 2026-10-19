import { MonitorX } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import zhCNCommon from "@/i18n/locales/zh-CN/common";

export function RuntimeUnavailable({ className }: { className?: string }) {
  const { i18n, t } = useTranslation("common");
  const title = i18n.isInitialized
    ? t("runtimeUnavailable.title")
    : zhCNCommon.runtimeUnavailable.title;
  const description = i18n.isInitialized
    ? t("runtimeUnavailable.description")
    : zhCNCommon.runtimeUnavailable.description;
  return (
    <Empty className={className ?? "min-h-72 border"}>
      <EmptyHeader>
        <EmptyMedia variant="icon"><MonitorX /></EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}
