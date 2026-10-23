import type { Ref } from "react";
import { useTranslation } from "react-i18next";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { ContactChannel } from "@/lib/creator";
import { contactChannelLabel } from "./i18n-labels";

const CHANNELS: ContactChannel[] = [
  "email",
  "wechat",
  "x",
  "instagram",
  "telegram",
  "youtube",
  "tiktok",
  "website",
  "link_hub",
];

export function ContactRouteFields({
  idPrefix,
  channel,
  value,
  sourceUrl,
  pending,
  error,
  valueRef,
  onChannelChange,
  onValueChange,
  onSourceUrlChange,
}: {
  idPrefix: string;
  channel: ContactChannel;
  value: string;
  sourceUrl: string;
  pending: boolean;
  error: string | null;
  valueRef?: Ref<HTMLInputElement>;
  onChannelChange: (value: ContactChannel) => void;
  onValueChange: (value: string) => void;
  onSourceUrlChange: (value: string) => void;
}) {
  const { t } = useTranslation("leads");
  const channelId = `${idPrefix}-channel`;
  const valueId = `${idPrefix}-value`;
  const sourceId = `${idPrefix}-source`;
  const errorId = `${idPrefix}-error`;

  return (
    <>
      <div className="grid gap-2">
        <Label htmlFor={channelId}>{t("contactRoutes.fields.channel")}</Label>
        <Select value={channel} onValueChange={(next) => onChannelChange(next as ContactChannel)} disabled={pending}>
          <SelectTrigger id={channelId}><SelectValue /></SelectTrigger>
          <SelectContent>
            {CHANNELS.map((next) => (
              <SelectItem key={next} value={next}>{contactChannelLabel(t, next)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor={valueId}>{t("contactRoutes.fields.value")}</Label>
        <Input
          ref={valueRef}
          id={valueId}
          value={value}
          onChange={(event) => onValueChange(event.target.value)}
          placeholder={channel === "email" ? "creator@example.com" : t("contactRoutes.fields.valuePlaceholder")}
          aria-invalid={Boolean(error)}
          aria-describedby={error ? errorId : undefined}
          autoComplete="off"
          disabled={pending}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={sourceId}>{t("contactRoutes.fields.sourceUrl")}</Label>
        <Input
          id={sourceId}
          type="url"
          value={sourceUrl}
          onChange={(event) => onSourceUrlChange(event.target.value)}
          placeholder="https://…"
          autoComplete="off"
          disabled={pending}
        />
      </div>
      {error && <p id={errorId} role="alert" className="text-sm text-destructive">{error}</p>}
    </>
  );
}
