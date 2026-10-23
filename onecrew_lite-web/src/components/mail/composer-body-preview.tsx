import type { JSX } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";

/**
 * Builds the standalone HTML document rendered inside the preview iframe.
 *
 * The white `background:#fff` and fixed colors on `body` are deliberate: the
 * preview simulates what the recipient's mail client will show, so it must NOT
 * follow the desktop light/dark theme. Everything is inlined because the
 * iframe is fully sandboxed and cannot load external stylesheets.
 *
 * The CSP meta is a second line of defence behind `sandbox=""`: the sandbox
 * blocks scripts/forms/navigation but not sub-resource loads, so template html
 * could still fetch remote fonts/media or `<meta http-equiv=refresh>` away.
 * Only inline styles and images are allowed.
 */
export function buildPreviewDocument(html: string): string {
  return [
    "<!doctype html>",
    "<html>",
    "<head>",
    '<meta charset="utf-8">',
    "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data: http: https:; style-src 'unsafe-inline'\">",
    "<style>",
    "body{margin:0;padding:16px;font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,\"Helvetica Neue\",Arial,\"PingFang SC\",\"Hiragino Sans GB\",\"Microsoft YaHei\",sans-serif;font-size:15px;line-height:1.6;color:#1f1f1f;background:#fff}",
    "img{max-width:100%}",
    "table{border-collapse:collapse}",
    "td,th{border:1px solid #ddd;padding:4px 8px}",
    "blockquote{border-left:3px solid #ccc;margin:0;padding-left:12px;color:#555}",
    "</style>",
    "</head>",
    "<body>",
    html,
    "</body>",
    "</html>",
  ].join("\n");
}

/**
 * Preview tab content: shows what the recipient will receive.
 *
 * Rich HTML is rendered in an iframe with `sandbox=""` — no permissions at
 * all. With an empty sandbox, `<script>` inside `srcDoc` never executes, forms
 * cannot submit, and the frame cannot navigate or reach the parent window.
 * That is why we render the derived HTML (and MCP-authored template HTML)
 * without pulling in an HTML sanitizer.
 */
export function ComposerBodyPreview(props: {
  html: string | null;
  text: string;
  plainText: boolean;
  className?: string;
}): JSX.Element {
  const { t } = useTranslation("mail");
  const { html, text, plainText, className } = props;

  if (plainText || html === null) {
    return (
      <div className={cn("flex min-h-0 flex-1 flex-col overflow-auto", className)}>
        {text.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("editor.emptyPreview")}</p>
        ) : (
          <pre className="whitespace-pre-wrap break-words font-sans text-base leading-7">{text}</pre>
        )}
      </div>
    );
  }

  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>
      {/* The recipient-client white ground lives in the srcdoc body (see buildPreviewDocument). */}
      <iframe
        title={t("editor.previewTitle")}
        sandbox=""
        srcDoc={buildPreviewDocument(html)}
        className="h-full w-full border-0"
      />
    </div>
  );
}
