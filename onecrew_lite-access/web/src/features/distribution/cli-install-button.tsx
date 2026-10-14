import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";
import { createCliInstallLink, restoreCliInstallLink, DistributionApiError, type DistributionApplication, type CliInstallLink } from "@/lib/distribution-api";

const targetNames = {"aarch64-apple-darwin": "macOS · Apple Silicon", "x86_64-apple-darwin": "macOS · Intel", "x86_64-pc-windows-msvc": "Windows · x64"};
export function CliInstallButton({release}: {release: DistributionApplication["cliRelease"]}) {
  const { locale } = useAccessI18n();
  const en = locale === "en";
  const [pending, setPending] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [link, setLink] = useState<CliInstallLink | null>(null);
  const [restoring, setRestoring] = useState(true);
  const generation = useRef(0);
  const [now, setNow] = useState(Date.now);
  const [retryAt, setRetryAt] = useState(0);
  useEffect(() => {
    const current = ++generation.current;
    setLink(null); setError(null); setCopied(false); setPending(false);
    if (release.status !== "published") { setRestoring(false); return; }
    setRestoring(true);
    void restoreCliInstallLink().then(value => {
      if (generation.current === current) setLink(value.link);
    }).catch(cause => {
      if (generation.current !== current) return;
      if (cause instanceof DistributionApiError && cause.code === "rate_limited") {
        setRetryAt(Date.now() + (cause.retryAfter ?? 60) * 1000);
        setError(en ? "Please wait before trying again." : "请求过于频繁，请等待后重试。");
      } else setError(en ? "Unable to restore the installation prompt. Try generating it again." : "暂时无法恢复安装提示词，请点击生成重试。");
    }).finally(() => { if (generation.current === current) setRestoring(false); });
    return () => { generation.current++; };
  }, [release.status, en]);
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(timer); }, []);
  useEffect(() => { if (!copied) return; const timer = setTimeout(() => setCopied(false), 2500); return () => clearTimeout(timer); }, [copied]);
  const reusable = !!link && link.expiresAt > now;
  const waitSeconds = Math.max(0, Math.ceil((retryAt - now) / 1000));
  const expiresAt = link?.expiresAt;
  const prompt = link?.prompt;
  async function copy() {
    const current = generation.current;
    setError(null); setPending(true); setCopied(false);
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard");
      const value = link && link.expiresAt > Date.now() ? link : await createCliInstallLink();
      if (generation.current !== current) return;
      setLink(value);
      setNow(Date.now());
      await navigator.clipboard.writeText(value.prompt);
      if (generation.current !== current) return;
      setCopied(true);
    } catch (cause) {
      if (generation.current !== current) return;
      if (cause instanceof DistributionApiError && cause.code === "rate_limited") {
        setRetryAt(Date.now() + (cause.retryAfter ?? 60) * 1000);
        setError(en ? "Please wait before generating another prompt." : "生成过于频繁，请等待后再生成提示词。");
      } else setError(cause instanceof DistributionApiError && cause.code === "release_unavailable" ? (en ? "The CLI release is unavailable. Refresh the status and try again." : "CLI 版本暂时不可用，请刷新状态后重试。") : (en ? "Unable to copy. Please try again; a generated link is kept on this page." : "复制失败，请重试；已生成的链接会保留在当前页面。"));
    } finally { if (generation.current === current) setPending(false); }
  }
  return <div className="distribution-cli">
    {release.status === "published" ? <><p className="distribution-release-summary">OneCrew CLI {release.version}</p><p>{release.targets.map(target => targetNames[target]).join(" / ")}</p></> : <p role="status">{release.status === "not_published" ? (en ? "The CLI release has not been published yet." : "CLI 版本尚未发布。") : (en ? "The CLI release is unavailable. Refresh the status to try again." : "CLI 版本暂时不可用，请刷新状态重试。")}</p>}
    {prompt && reusable ? <div className="distribution-prompt">
      <div className="distribution-prompt-heading"><strong>{en ? "Installation prompt" : "安装提示词"}</strong><Button size="sm" variant="outline" disabled={pending} onClick={() => void copy()}>{copied ? (en ? "Installation prompt copied" : "已复制安装提示词") : (en ? "Copy installation prompt again" : "再次复制安装提示词")}</Button></div>
      <pre><code>{prompt}</code></pre>
    </div> : <Button disabled={restoring || pending || release.status !== "published" || waitSeconds > 0} onClick={() => void copy()}>{restoring ? (en ? "Restoring…" : "恢复中…") : pending ? (en ? "Generating…" : "生成中…") : (en ? "Generate and copy installation prompt" : "生成并复制安装提示词")}</Button>}

    {expiresAt && <p role="status">{expiresAt <= now ? (en ? "Previous prompt expired: " : "上次提示词已过期：") : (en ? "Private link expires: " : "私人链接有效期至：")}{new Date(expiresAt).toLocaleString(en ? "en-US" : "zh-CN")}{expiresAt > now && <span>{en ? ` (${Math.ceil((expiresAt-now)/60000)} minutes remaining)` : `（剩余 ${Math.ceil((expiresAt-now)/60000)} 分钟）`}</span>}</p>}
    {waitSeconds > 0 && <p role="status">{en ? `Try again in ${waitSeconds} seconds.` : `请在 ${waitSeconds} 秒后重试。`}</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
