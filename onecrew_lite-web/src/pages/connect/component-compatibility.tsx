import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { checkComponentCompatibility } from "@/lib/component-compatibility";
import { formatEpochMilliseconds } from "@/lib/creator";

export function ComponentCompatibility({ version, hash }: { version: string; hash: string }) {
  const { t, i18n } = useTranslation("shell");
  // Local component inspection only. No platform search, installer or companion launch.
  const query = useQuery({ queryKey: ["component-compatibility", hash], queryFn: checkComponentCompatibility,
    retry: false, staleTime: 60_000, refetchOnWindowFocus: false });
  const report = query.data;
  return <details className="rounded-md border p-3 text-sm">
    <summary className="cursor-pointer font-medium">{t("compatibility.title")} · {version} · {hash.slice(0, 12)}</summary>
    <div className="space-y-3 pt-3">
      {report && <>
        <p>{t("compatibility.builtAt", { time: formatEpochMilliseconds(BigInt(report.built_at) * 1000n, i18n.resolvedLanguage, { year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", second: "numeric" }) })}</p>
        <dl className="grid grid-cols-2 gap-2">
          {[
            ["onecrew_lite", report.version, "ready"],
            ["Codex", report.plugin_version, report.plugin_state],
            ["Library", report.library_version, report.library_state],
            ["OpenCLI", report.opencli_version, report.opencli_state],
          ].map(([name, value, state]) => <div key={name}><dt className="text-muted-foreground">{name}</dt>
            <dd className="break-words">{value ?? "—"} · {t(state === "ready" ? "compatibility.ready" : "compatibility.attention")}</dd>
            {state !== "ready" && <p className="text-xs text-muted-foreground">{state}</p>}
          </div>)}
        </dl>
        <p className="text-xs text-muted-foreground">{t("compatibility.checkedAt", { time: formatEpochMilliseconds(BigInt(report.checked_at) * 1000n, i18n.resolvedLanguage, { hour: "numeric", minute: "numeric", second: "numeric" }) })}</p>
      </>}
      {query.isError && <p role="alert">{t("compatibility.failed")}</p>}
      <Button size="sm" variant="outline" disabled={query.isFetching} onClick={() => { void query.refetch(); }}>{t(query.isFetching ? "compatibility.checking" : "compatibility.check")}</Button>
    </div>
  </details>;
}
