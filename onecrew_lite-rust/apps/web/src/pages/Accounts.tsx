import { useTranslation } from "react-i18next";
import { GmailConnection } from "@/components/mail/gmail-connection";
import { RuntimeUnavailable } from "@/components/runtime-unavailable";
import { isRuntimeAvailable } from "@/lib/local-runtime";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Connect } from "@/pages/Connect";

export function Accounts({ onConnections }: { onConnections: () => void }) {
  const { t } = useTranslation("shell");
  if (!isRuntimeAvailable()) return <RuntimeUnavailable />;
  return <Tabs defaultValue="email" className="mx-auto flex h-full min-h-0 w-full max-w-5xl flex-col gap-5">
    <TabsList variant="line" className="h-auto shrink-0 justify-start gap-5 rounded-none border-b bg-transparent p-0 pb-1">
      <TabsTrigger value="email">{t("views.accounts.emailTab")}</TabsTrigger>
      <TabsTrigger value="connections">{t("views.accounts.connectionsTab")}</TabsTrigger>
    </TabsList>
    <TabsContent value="email" className="min-h-0 flex-1 overflow-auto"><GmailConnection /></TabsContent>
    <TabsContent value="connections" className="min-h-0 flex-1 overflow-auto">
      <Button variant="outline" onClick={onConnections}>{t("views.accounts.managePlatforms")}</Button>
      <Connect />
    </TabsContent>
  </Tabs>;
}
