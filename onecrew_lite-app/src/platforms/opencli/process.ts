import { spawn } from "node:child_process";
import { dirname, extname, isAbsolute } from "node:path";
import { OpenCliError, type OpenCliErrorCode } from "./errors.ts";

const MAX_STDOUT = 4 * 1024 * 1024;
const MAX_STDERR = 32 * 1024;
export const OPENCLI_PROBE_TIMEOUT = 15_000;
export const OPENCLI_READ_TIMEOUT = 45_000;

export interface TrustedOpenCliProcess {
  readonly executable: string;
  readonly script?: string;
}
interface RunOptions { readonly deadlineMs: number; readonly signal?: AbortSignal; readonly webDiagnostics?: boolean; readonly foregroundBrowser?: boolean; readonly installation?: boolean; readonly configurationDirectory?: string }

export function classifyOpenCliFailure(exitCode: number | null, stdout: Uint8Array, stderr: Uint8Array): OpenCliError {
  const output = `${new TextDecoder().decode(stdout)} ${new TextDecoder().decode(stderr)}`.toLowerCase();
  const families = ["onecrew_instagram_search", "onecrew_reddit_profile", "onecrew_pinterest", "onecrew_linkedin", "onecrew_fb_profile", "onecrew_fb_contact"];
  if (families.some(family => output.includes(`${family}_risk`))) return new OpenCliError("opencli_risk_control");
  if (families.some(family => output.includes(`${family}_auth`))) return new OpenCliError("opencli_auth_required");
  if (exitCode === 77 || ["auth_required", "not logged in", "http 401 - make sure you are logged in to instagram"].some(marker => output.includes(marker))) return new OpenCliError("opencli_auth_required");
  if (["captcha", "rate limit", "rate-limit", "too many requests", "http 429", "status 429", "verify you are human", "unusual traffic", "sogou weixin blocked this search request", "linkedin verification requires human review", "douyin verification requires human review", "pixiv verification requires human review", "facebook verification requires human review"].some(marker => output.includes(marker))) return new OpenCliError("opencli_risk_control");
  const native = classifyNativeReadFailure(output);
  if (native) return native;
  if (["onecrew_instagram_recent_source_", "onecrew_instagram_recent_invalid_", "invalid native pinterest input or result", "onecrew_linkedin_invalid", "onecrew_weibo_profile_invalid", "onecrew_weibo_followers_invalid", "onecrew_weibo_id_conflict", "onecrew_weibo_id_invalid", "onecrew_weibo_recent_invalid", "onecrew_weibo_recent_id_invalid", "onecrew_douyin_invalid", "onecrew_pixiv_invalid", "onecrew_pinterest_invalid", "onecrew_pinterest_read_input", "onecrew_pinterest_read_source", "onecrew_pinterest_read_session", "onecrew_pinterest_read_response", "onecrew_pinterest_read_resource", "onecrew_facebook_source_invalid", "onecrew_facebook_source_ambiguous", "onecrew_facebook_comments_invalid", "onecrew_facebook_comments_unavailable", "onecrew_fb_recent_invalid", "onecrew_fb_recent_unavailable", "onecrew_fb_identity_invalid", "onecrew_fb_profile_invalid", "onecrew_fb_contact_invalid", "onecrew_fb_original_unavailable"].some(marker => output.includes(marker))) return new OpenCliError("opencli_invalid_output");
  if (exitCode === 75 && output.split(/\r?\n/u).some(line => line.trim() === "code: timeout") || output.includes("onecrew_instagram_search_timeout")) return new OpenCliError("opencli_timeout");
  return new OpenCliError("opencli_command_failed", exitCode);
}
function classifyNativeReadFailure(output: string): OpenCliError | null {
  if (output.includes("onecrew_youtube_search_bootstrap_timeout")) return new OpenCliError("opencli_timeout");
  const stages: readonly (readonly [string, OpenCliErrorCode])[] = [
    ["onecrew_linkedin_invalid_contact_profile_route", "linkedin_contact_profile_route"],
    ["onecrew_linkedin_invalid_contact_dialog_owner", "linkedin_contact_dialog_owner_conflict"],
    ["onecrew_linkedin_invalid_contact_owner", "linkedin_contact_owner_conflict"],
    ["onecrew_linkedin_invalid_contact_before_click", "linkedin_contact_before_click"],
    ["onecrew_linkedin_invalid_contact_projection", "linkedin_contact_projection_invalid"],
    ["onecrew_linkedin_invalid_contact_unavailable", "linkedin_contact_unavailable"],
    ["onecrew_linkedin_content_unavailable", "linkedin_content_unavailable"],
    ["onecrew_linkedin_profile_unavailable", "linkedin_profile_unavailable"],
    ["onecrew_fb_profile_timeout", "facebook_profile_timeout"],
    ["onecrew_fb_profile_invalid", "facebook_profile_layout_invalid"],
    ["onecrew_fb_contact_invalid", "facebook_contact_layout_invalid"],
    ["onecrew_youtube_search_source_invalid", "provider_response_redirected"],
    ["youtube search bootstrap data not found", "provider_response_shape_invalid"],
    ["youtube search returned malformed result rows", "provider_response_shape_invalid"],
    ["youtube search repeated a continuation cursor", "provider_response_shape_invalid"],
    ["youtube search pagination stopped before satisfying the requested limit", "provider_response_shape_invalid"],
    ["youtube search http", "provider_response_http_error"],
    ["onecrew_instagram_search_read_http_404", "provider_endpoint_not_found"],
    ["onecrew_instagram_search_read_http", "provider_response_http_error"],
    ["onecrew_instagram_search_read_redirect", "provider_response_redirected"],
    ["onecrew_instagram_search_read_json", "provider_response_invalid_json"],
    ["onecrew_instagram_search_read", "provider_request_failed"],
    ["onecrew_reddit_profile_inaccessible", "provider_source_unavailable"],
    ["onecrew_reddit_profile_read_http", "provider_response_http_error"],
    ["onecrew_reddit_profile_read_redirect", "provider_response_redirected"],
    ["onecrew_reddit_profile_read_json", "provider_response_invalid_json"],
    ["onecrew_reddit_profile_read", "provider_request_failed"],
    ["onecrew_pinterest_read_response_http", "provider_response_http_error"],
    ["onecrew_pinterest_read_response_encoding", "provider_response_invalid_json"],
    ["onecrew_pinterest_read_resource", "provider_response_shape_invalid"],
    ["onecrew_pinterest_invalid", "provider_response_shape_invalid"],
    ["onecrew_youtube_video_unavailable", "youtube_video_unavailable"],
    ["onecrew_youtube_layout_unsupported", "youtube_watch_layout_unsupported"],
    ["onecrew_youtube_identity_invalid", "youtube_author_identity_invalid"],
    ["onecrew_youtube_description_unavailable", "youtube_description_unavailable"],
    ["onecrew_instagram_search_invalid", "opencli_invalid_output"],
    ["onecrew_instagram_search_input", "opencli_invalid_output"],
    ["onecrew_reddit_profile_invalid", "opencli_invalid_output"],
    ["onecrew_reddit_profile_input", "opencli_invalid_output"],
  ];
  const stage = stages.find(([marker]) => output.includes(marker));
  return stage ? new OpenCliError(stage[1]) : null;
}

// 仅内部进程边界可接收 argv；产品门面必须使用固定的已审核读取命令。
// stderr 只供当前适配器核对来源，不进入公开错误或状态。
export function runTrustedOpenCliProcess(installation: TrustedOpenCliProcess, args: readonly string[], options: RunOptions): Promise<Readonly<{ stdout: string; stderr: Buffer }>> {
  if (!installation.executable.isWellFormed() || installation.executable.includes("\0") || !isAbsolute(installation.executable) || [".cmd", ".bat", ".ps1"].includes(extname(installation.executable).toLowerCase()) || installation.script !== undefined && (!isAbsolute(installation.script) || !installation.script.isWellFormed() || installation.script.includes("\0")) || !Number.isInteger(options.deadlineMs) || options.deadlineMs < 1 || options.deadlineMs > (options.installation ? 240000 : OPENCLI_READ_TIMEOUT) || args.some(value => typeof value !== "string" || !value.isWellFormed() || value.includes("\0"))) throw new OpenCliError("opencli_invalid_input");
  if (options.signal?.aborted) return Promise.reject(new OpenCliError("opencli_cancelled"));
  if (options.configurationDirectory !== undefined && (!isAbsolute(options.configurationDirectory) || !options.configurationDirectory.isWellFormed() || options.configurationDirectory.includes("\0"))) throw new OpenCliError("opencli_invalid_input");
  return new Promise((resolve, reject) => {
    const child = spawn(installation.executable, [...(installation.script === undefined ? [] : [installation.script]), ...args], {
      shell: false, windowsHide: true, detached: false, stdio: ["ignore", "pipe", "pipe"],
      ...(options.foregroundBrowser || options.configurationDirectory ? { env: { ...process.env, ...(options.foregroundBrowser ? { OPENCLI_WINDOW: "foreground" } : {}), ...(options.configurationDirectory ? { OPENCLI_CONFIG_DIR: options.configurationDirectory, HOME: dirname(options.configurationDirectory), ...(process.platform === "win32" ? { USERPROFILE: dirname(options.configurationDirectory) } : {}) } : {}) } } : {}),
      ...(process.platform === "win32" && installation.script ? { cwd: dirname(installation.script) } : {}),
    });
    let failure: OpenCliError | undefined;
    let settled = false;
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let stdoutSize = 0;
    let stderrSize = 0;
    let line: number[] = [];
    let diagnostics = false;
    let skipNetwork = false;
    const network = ["  GET ", "  POST ", "  PUT ", "  PATCH ", "  DELETE ", "  HEAD ", "  OPTIONS ", "  CONNECT ", "  TRACE "].map(value => Buffer.from(value));
    const stop = (error: OpenCliError) => {
      if (settled || failure) return;
      failure = error;
      child.kill("SIGKILL");
      child.stdout.destroy(); child.stderr.destroy();
    };
    let cooperativeReason: "opencli_cancelled" | "opencli_timeout" | null = null;
    let forceStop: ReturnType<typeof setTimeout> | undefined;
    const requestStop = (reason: "opencli_cancelled" | "opencli_timeout") => {
      if (!options.foregroundBrowser) { stop(new OpenCliError(reason)); return; }
      if (settled || failure) return;
      cooperativeReason = reason;
      failure = new OpenCliError("opencli_cancel_pending");
      child.kill("SIGTERM");
      forceStop = setTimeout(() => { child.kill("SIGKILL"); child.stdout.destroy(); child.stderr.destroy(); }, 5000);
    };
    const cancel = () => requestStop("opencli_cancelled");
    const deadline = setTimeout(() => requestStop("opencli_timeout"), options.deadlineMs);
    const cleanup = () => { settled = true; clearTimeout(deadline); clearTimeout(forceStop); options.signal?.removeEventListener("abort", cancel); };
    options.signal?.addEventListener("abort", cancel, { once: true });
    child.stdout.on("data", (chunk: Buffer) => {
      if (failure) return;
      stdoutSize += chunk.length;
      if (stdoutSize > MAX_STDOUT) stop(new OpenCliError("opencli_output_too_large"));
      else stdout.push(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      if (failure && failure.code !== "opencli_cancel_pending") return;
      if (!options.webDiagnostics) {
        stderrSize += chunk.length;
        if (stderrSize > MAX_STDERR) stop(new OpenCliError("opencli_output_too_large"));
        else stderr.push(chunk);
        return;
      }
      for (const byte of chunk) {
        if (skipNetwork) { if (byte === 10) skipNetwork = false; continue; }
        line.push(byte);
        if (diagnostics && network.some(prefix => prefix.length === line.length && prefix.every((value, index) => value === line[index]))) { line = []; skipNetwork = true; continue; }
        if (stderrSize + line.length > MAX_STDERR) { stop(new OpenCliError("opencli_output_too_large")); return; }
        if (byte === 10) {
          const kept = Buffer.from(line);
          if (kept.equals(Buffer.from("[web-read diagnose]\n")) || kept.equals(Buffer.from("[web-read diagnose]\r\n"))) diagnostics = true;
          stderr.push(kept); stderrSize += kept.length; line = [];
        }
      }
    });
    child.stdout.on("error", () => stop(new OpenCliError("opencli_invalid_output")));
    child.stderr.on("error", () => stop(new OpenCliError("opencli_invalid_output")));
    child.once("error", () => { if (!settled) { cleanup(); reject(failure ?? new OpenCliError("opencli_unavailable")); } });
    child.once("close", code => {
      if (settled) return;
      cleanup();
      if (failure) {
        const closed = cooperativeReason !== null && Buffer.concat(stderr).toString("utf8").split(/\r?\n/u).includes("ONECREW_BROWSER_CANCELLED_CLOSED");
        reject(closed ? new OpenCliError(cooperativeReason!) : failure); return;
      }
      if (line.length) stderr.push(Buffer.from(line));
      const output = Buffer.concat(stdout);
      const diagnostic = Buffer.concat(stderr);
      if (code !== 0) { reject(classifyOpenCliFailure(code, output, diagnostic)); return; }
      Promise.resolve().then(() => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(output)).then(
        text => resolve(Object.freeze({ stdout: text, stderr: diagnostic })),
        error => { if (error instanceof TypeError) reject(new OpenCliError("opencli_invalid_output")); else reject(error); },
      );
    });
  });
}
