import { useState } from "react";
import { invoke } from "@/lib/local-runtime";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { type Account } from "@/types";

/** A ProxyConfig as the backend commands expect it. */
export type ProxyConfig = {
  kind: "http" | "https" | "socks5";
  host: string;
  port: number;
  username: string | null;
  password: string | null;
};

export function toProxyConfig(
  type: string,
  ip: string,
  port: string,
  user: string,
  pass: string,
): ProxyConfig {
  return {
    kind: type.toLowerCase() as ProxyConfig["kind"],
    host: ip.trim(),
    port: Number(port),
    username: user.trim() || null,
    password: pass || null,
  };
}

/** Result of `probe_proxy`: egress identity + the 家宽-aligned soft fingerprint
 *  (timezone from the IP's own geolocation, language from CLDR). */
export type ProxyProbe = {
  ip: string;
  country: string;
  timezone: string | null;
  accept_language: string | null;
  endpoint: string;
};

export const PROXY_TYPES = ["HTTP", "HTTPS", "SOCKS5"] as const;
/** Telegram (grammers) only speaks SOCKS5 — no HTTP/HTTPS proxy path exists. */
export const SOCKS5_ONLY = ["SOCKS5"] as const;

/** Shared residential-proxy input fields (wizard + per-account dialog). `types`
 *  restricts the selectable protocols (Telegram is SOCKS5-only); one option locks
 *  the selector. */
export function ProxyFields({
  type,
  setType,
  ip,
  setIp,
  port,
  setPort,
  user,
  setUser,
  pass,
  setPass,
  onPaste,
  onAnyChange,
  types = PROXY_TYPES,
}: {
  type: string;
  setType: (v: string) => void;
  ip: string;
  setIp: (v: string) => void;
  port: string;
  setPort: (v: string) => void;
  user: string;
  setUser: (v: string) => void;
  pass: string;
  setPass: (v: string) => void;
  onPaste: (v: string) => void;
  onAnyChange?: () => void;
  types?: readonly string[];
}) {
  const { t } = useTranslation("accounts");
  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="proxy-paste">{t("proxy.fields.quickPaste")}</Label>
        <Input
          id="proxy-paste"
          placeholder={t("proxy.fields.quickPastePlaceholder")}
          className="font-mono text-xs"
          onChange={(e) => onPaste(e.target.value)}
        />
      </div>
      <div className="grid grid-cols-3 gap-3">
        <div className="space-y-2">
          <Label htmlFor="proxy-type">{t("proxy.fields.type")}</Label>
          <Select
            value={type}
            onValueChange={(v) => {
              setType(v || types[0]);
              onAnyChange?.();
            }}
          >
            <SelectTrigger id="proxy-type" className="w-full" disabled={types.length <= 1}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {types.map((t) => (
                <SelectItem key={t} value={t}>
                  {t}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="col-span-2 space-y-2">
          <Label htmlFor="proxy-ip">{t("proxy.fields.ip")}</Label>
          <Input
            id="proxy-ip"
            value={ip}
            onChange={(e) => {
              setIp(e.target.value);
              onAnyChange?.();
            }}
            placeholder={t("proxy.fields.ipPlaceholder")}
          />
        </div>
      </div>
      <div className="grid grid-cols-3 gap-3">
        <div className="space-y-2">
          <Label htmlFor="proxy-port">{t("proxy.fields.port")}</Label>
          <Input
            id="proxy-port"
            value={port}
            onChange={(e) => {
              setPort(e.target.value);
              onAnyChange?.();
            }}
            placeholder={t("proxy.fields.portPlaceholder")}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="proxy-user">{t("proxy.fields.usernameOptional")}</Label>
          <Input
            id="proxy-user"
            value={user}
            onChange={(e) => {
              setUser(e.target.value);
              onAnyChange?.();
            }}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="proxy-pass">{t("proxy.fields.passwordOptional")}</Label>
          <Input
            id="proxy-pass"
            type="password"
            value={pass}
            onChange={(e) => {
              setPass(e.target.value);
              onAnyChange?.();
            }}
          />
        </div>
      </div>
    </div>
  );
}

/** Per-profile residential (家宽) proxy setup for an existing account. */
export function ProxyDialog({
  account,
  open,
  onOpenChange,
  onChanged,
}: {
  account: Account;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: () => void;
}) {
  const { t } = useTranslation("accounts");
  const socks5Only = account.platform === "telegram";
  const [type, setType] = useState(socks5Only ? "SOCKS5" : "HTTP");
  const [ip, setIp] = useState(account.proxy?.split(":")[0] ?? "");
  const [port, setPort] = useState(account.proxy?.split(":")[1] ?? "");
  const [user, setUser] = useState("");
  const [pass, setPass] = useState("");
  const [busy, setBusy] = useState(false);

  function onPaste(value: string) {
    const parts = value.split(":");
    if (parts.length >= 2) {
      setIp(parts[0] ?? "");
      setPort(parts[1] ?? "");
      setUser(parts[2] ?? "");
      setPass(parts[3] ?? "");
    }
  }

  async function detect() {
    setBusy(true);
    try {
      const probe = await invoke<ProxyProbe>("probe_proxy", {
        proxy: toProxyConfig(type, ip, port, user, pass),
      });
      toast.success(t("proxy.available"), {
        description: t("proxy.toastDescription", {
          ip: probe.ip,
          country: probe.country ? ` · ${probe.country}` : "",
          endpoint: probe.endpoint,
        }),
      });
    } catch (error) {
      toast.error(t("proxy.toasts.detectionFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    setBusy(true);
    try {
      await invoke("set_profile_proxy", {
        profileId: account.id,
        proxy: toProxyConfig(type, ip, port, user, pass),
      });
      toast.success(t("proxy.toasts.saved"), {
        description: t("proxy.toasts.savedDescription", {
          username: account.username,
          address: `${ip}:${port}`,
        }),
      });
      onOpenChange(false);
      onChanged();
    } catch (error) {
      toast.error(t("proxy.toasts.saveFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    try {
      await invoke("clear_profile_proxy", { profileId: account.id });
      toast(t("proxy.toasts.cleared"));
      onOpenChange(false);
      onChanged();
    } catch (error) {
      toast.error(t("proxy.toasts.clearFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("proxy.dialog.title")}</DialogTitle>
          <DialogDescription>
            {t("proxy.dialog.description", { username: account.username })}
          </DialogDescription>
        </DialogHeader>
        <div className="py-2">
          <ProxyFields
            type={type}
            setType={setType}
            ip={ip}
            setIp={setIp}
            port={port}
            setPort={setPort}
            user={user}
            setUser={setUser}
            pass={pass}
            setPass={setPass}
            onPaste={onPaste}
            types={socks5Only ? SOCKS5_ONLY : PROXY_TYPES}
          />
        </div>
        <DialogFooter className="sm:justify-between">
          <Button variant="ghost" onClick={clear} disabled={busy}>
            {t("proxy.dialog.clear")}
          </Button>
          <div className="flex gap-2">
            <Button variant="outline" onClick={detect} disabled={!ip || !port || busy}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("proxy.detect")}
            </Button>
            <Button onClick={save} disabled={!ip || !port || busy}>
              {t("proxy.dialog.saveAndEnable")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
