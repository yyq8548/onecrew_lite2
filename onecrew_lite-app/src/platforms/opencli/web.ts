import { OpenCliError } from "./errors.ts";
import { publicWebUrl, validatePublicWebUrl } from "./public-url.ts";

export interface PublicOpenCliWebRead { requested_url: string; document_url: string; title: string; markdown_excerpt: string; truncated: boolean }
// 只解码原 CLI 固定 stdout/diagnose 协议；Markdown 正文保持原文，不解释其语法。
export async function decodePublicWebRead(requested: string, stdout: string, stderr: Buffer): Promise<PublicOpenCliWebRead> {
  validatePublicWebUrl(requested);
  const diagnostic = await Promise.resolve().then(() => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(stderr)).then(value => value, error => {
    if (error instanceof TypeError) throw new OpenCliError("opencli_invalid_output");
    throw error;
  });
  const blocks = diagnostic.split("[web-read diagnose]\n").slice(1);
  const first = blocks[0]?.split(/\r?\n/u)[0];
  const destination = first?.startsWith("url: ") ? first.slice(5) : "";
  if (blocks.length !== 1 || !publicWebUrl(destination) || !stdout.isWellFormed()) throw new OpenCliError("opencli_invalid_output");
  const normalized = stdout.replace(/\r\n/gu, "\n");
  const separator = normalized.indexOf("\n\n---\n\n");
  if (separator < 0) throw new OpenCliError("opencli_invalid_output");
  const header = normalized.slice(0, separator).split("\n");
  const body = normalized.slice(separator + "\n\n---\n\n".length);
  const title = header[0]?.startsWith("# ") ? header[0].slice(2) : "";
  const sources = header.filter(line => line.startsWith("> 原文链接: ")).map(line => line.slice("> 原文链接: ".length));
  if (!title.trim() || [...title].length > 500 || sources.length !== 1 || sources[0] !== requested || !body.trim() || body.includes("\0")) throw new OpenCliError("opencli_invalid_output");
  const gate = title.trim().replace(/[.…!]+$/gu, "").toLowerCase();
  if (["just a moment", "verify you are human", "checking your browser", "security verification", "安全验证", "安全驗證", "人机验证", "人機驗證", "un momento", "un instant", "einen moment", "um momento", "подождите", "しばらくお待ちください", "잠시만 기다려 주세요"].includes(gate) || Buffer.byteLength(body) < 3000 && ["performing security verification", "enable javascript and cookies to continue", "checking your browser before", "请完成人机验证", "請完成人機驗證"].some(phrase => body.toLowerCase().includes(phrase))) throw new OpenCliError("opencli_risk_control");
  let bytes = 0;
  let excerpt = "";
  for (const character of body) { const length = Buffer.byteLength(character); if (bytes + length > 20000) break; excerpt += character; bytes += length; }
  return { requested_url: requested, document_url: destination, title, markdown_excerpt: excerpt, truncated: bytes < Buffer.byteLength(body) };
}
