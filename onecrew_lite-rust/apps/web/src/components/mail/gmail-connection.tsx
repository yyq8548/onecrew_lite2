import { useState } from "react";
import { RefreshCw, Plus } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { creatorInvoke } from "@/lib/creator";
import { Button } from "@/components/ui/button";
import { MAILBOX_QUERY_KEY } from "@/components/mail/queries";

type Connection = { id: string; provider: string; email: string; status: string };

export function GmailConnection() {
  const { t } = useTranslation("mail");
  const queryClient = useQueryClient();
  const [url, setUrl] = useState<string | null>(null);
  const connections = useQuery({
    queryKey: ["mail-access-connections"],
    queryFn: () => creatorInvoke<Connection[]>("onecrew_mail_access_connections"),
    retry: false,
  });
  const connect = useMutation({
    mutationFn: () => creatorInvoke<string>("onecrew_gmail_connect_url"),
    onMutate: () => setUrl(null),
    onSuccess: setUrl,
  });
  const refresh = useMutation({
    mutationFn: () => creatorInvoke<Connection[]>("onecrew_gmail_refresh"),
    onSuccess: async data => {
      setUrl(null);
      queryClient.setQueryData(["mail-access-connections"], data);
      await queryClient.invalidateQueries({ queryKey: MAILBOX_QUERY_KEY });
    },
  });
  return (
    <div className="space-y-3 border-b bg-card p-5 text-sm">
      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={refresh.isPending || connect.isPending} onClick={() => refresh.mutate()}>
          <RefreshCw className={refresh.isPending ? "animate-spin" : ""} />
          {t(refresh.isPending ? "gmail.refreshing" : "gmail.refresh")}
        </Button>
        <Button size="sm" variant="outline" disabled={connections.isPending || connections.isError || connect.isPending || refresh.isPending} onClick={() => connect.mutate()}>
          <Plus />{t(connect.isPending ? "gmail.connecting" : connections.data?.some(c => c.provider === "google") ? "gmail.reauthorize" : "gmail.connect")}
        </Button>
      </div>
      {url && <a className="block underline" href={url} target="_blank" rel="noreferrer">{t("gmail.authorize")}</a>}
      {url && <p className="text-muted-foreground">{t("gmail.expires")}</p>}
      {refresh.isSuccess && <p role="status">{t("gmail.refreshed")}</p>}
      {(connect.isError || connections.isError || refresh.isError) && <p role="alert">{t("gmail.failed")}</p>}
      {connections.data?.filter(c => c.provider === "google").map(c => (
        <p key={c.id}>{c.email} · {c.status === "connected" ? t("gmail.connected") : t("gmail.reconnect")}</p>
      ))}
      <p className="text-muted-foreground">{t("gmail.authorizationOnly")}</p>
      <p className="text-muted-foreground">{t("gmail.switchHint")}</p>
      <details className="rounded-md border p-3">
        <summary className="cursor-pointer font-medium">{t("gmail.identityHelp")}</summary>
        <p className="mt-2 text-muted-foreground">{t("gmail.identitySteps")}</p>
        <a className="mt-2 inline-block underline" href="https://access.onecrewai.io/account" target="_blank" rel="noopener noreferrer">{t("gmail.openAccount")}</a>
      </details>
    </div>
  );
}
