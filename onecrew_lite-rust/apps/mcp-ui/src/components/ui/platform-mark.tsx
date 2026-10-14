import { Globe } from "lucide-react";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { displayLabel } from "@mcp-ui/lib/display-label";
// Package-local copy of the canonical brand glyphs in apps/web/src/components/platform.tsx.
// Keep MCP App dependencies isolated; never import Web components or its theme.
// Channel identity lives in the `.channel-mark` platform palette in theme.css;
// a glyph carries only its path here, never a second colour vocabulary.
const platforms = {
  "x": {
    "label": "X",
    "path": "M18.244 2.25h3.308l-7.227 8.26 8.502 11.24h-6.657l-5.214-6.817-5.967 6.817H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231 5.45-6.231Zm-1.161 17.52h1.833L7.084 4.126H5.117L17.083 19.77Z"
  },
  "instagram": {
    "label": "Instagram",
    "path": "M12 2.163c3.204 0 3.584.012 4.85.07 3.252.148 4.771 1.691 4.919 4.919.058 1.265.069 1.645.069 4.849 0 3.205-.012 3.584-.069 4.849-.149 3.225-1.664 4.771-4.919 4.919-1.266.058-1.644.07-4.85.07-3.204 0-3.584-.012-4.849-.07-3.26-.149-4.771-1.699-4.919-4.92-.058-1.265-.07-1.644-.07-4.849 0-3.204.013-3.583.07-4.849.149-3.227 1.664-4.771 4.919-4.919 1.266-.057 1.645-.069 4.849-.069zm0-2.163c-3.259 0-3.667.014-4.947.072-4.358.2-6.78 2.618-6.98 6.98-.059 1.281-.073 1.689-.073 4.948 0 3.259.014 3.668.072 4.948.2 4.358 2.618 6.78 6.98 6.98 1.281.058 1.689.072 4.948.072 3.259 0 3.668-.014 4.948-.072 4.354-.2 6.782-2.618 6.979-6.98.059-1.28.073-1.689.073-4.948 0-3.259-.014-3.667-.072-4.947-.196-4.354-2.617-6.78-6.979-6.98-1.281-.059-1.69-.073-4.949-.073zm0 5.838c-3.403 0-6.162 2.759-6.162 6.162s2.759 6.163 6.162 6.163 6.162-2.759 6.162-6.163c0-3.403-2.759-6.162-6.162-6.162zm0 10.162c-2.209 0-4-1.79-4-4 0-2.209 1.791-4 4-4s4 1.791 4 4c0 2.21-1.791 4-4 4zm6.406-11.845c-.796 0-1.441.645-1.441 1.44s.645 1.44 1.441 1.44c.795 0 1.439-.645 1.439-1.44s-.644-1.44-1.439-1.44z"
  },
  "telegram": {
    "label": "Telegram",
    "path": "M21.94 4.6 18.9 19.02c-.23 1.01-.83 1.26-1.68.79l-4.64-3.42-2.24 2.16c-.25.25-.46.46-.94.46l.33-4.73L18.7 6.3c.37-.33-.08-.51-.58-.18L7.47 13.1l-4.57-1.43c-.99-.31-1.01-.99.21-1.47l17.87-6.89c.83-.3 1.55.2 1.28 1.29Z"
  },
  "youtube": {
    "label": "YouTube",
    "path": "M23.5 6.5a3 3 0 0 0-2.1-2.12C19.5 3.86 12 3.86 12 3.86s-7.5 0-9.4.52A3 3 0 0 0 .5 6.5C0 8.4 0 12 0 12s0 3.6.5 5.5a3 3 0 0 0 2.1 2.12c1.9.52 9.4.52 9.4.52s7.5 0 9.4-.52a3 3 0 0 0 2.1-2.12C24 15.6 24 12 24 12s0-3.6-.5-5.5ZM9.6 15.6V8.4l6.2 3.6-6.2 3.6Z"
  },
  "tiktok": {
    "label": "TikTok",
    "path": "M12.53.02C13.84 0 15.14.01 16.44 0c.08 1.53.63 3.09 1.75 4.17 1.12 1.11 2.7 1.62 4.24 1.79v4.03c-1.44-.05-2.89-.35-4.2-.97-.57-.26-1.1-.59-1.62-.93-.01 2.92.01 5.84-.02 8.75-.08 1.4-.54 2.79-1.35 3.94-1.31 1.92-3.58 3.17-5.91 3.21-1.43.08-2.86-.31-4.08-1.03-2.02-1.19-3.44-3.37-3.65-5.71-.02-.5-.03-1-.01-1.49.18-1.9 1.12-3.72 2.58-4.96 1.66-1.44 3.98-2.13 6.15-1.72.02 1.48-.04 2.96-.04 4.44-.99-.32-2.15-.23-3.02.37-.63.41-1.11 1.04-1.36 1.75-.21.51-.15 1.07-.14 1.61.24 1.64 1.82 3.02 3.5 2.87 1.12-.01 2.19-.66 2.77-1.61.19-.33.4-.67.41-1.06.1-1.79.06-3.57.07-5.36.01-4.03-.01-8.05.02-12.07z"
  }
} as const;

export function ChannelMark({ platform, label, compact = false }: { platform: string; label?: string; compact?: boolean }) {
  const { locale } = useAppLocale();
  const icon = platforms[platform as keyof typeof platforms];
  const name = label ?? icon?.label ?? displayLabel(platform === "email" ? "Email" : platform, locale);
  return <span className="inline-flex min-w-0 items-center gap-2" data-channel={platform}>
    <span role="img" aria-label={name} data-platform={platform} className={`channel-mark ${compact ? "size-5" : "size-[22px]"}`}>
      {platform === "email" ? <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden="true"><path fill="currentColor" d="M.05 3.555A2 2 0 0 1 2 2h12a2 2 0 0 1 1.95 1.555L8 8.414.05 3.555ZM0 4.697v7.104l5.803-3.558L0 4.697ZM6.761 8.83l-6.57 4.027A2 2 0 0 0 2 14h12a2 2 0 0 0 1.808-1.144L9.238 8.83 8 9.586 6.761 8.83Zm3.436-.586L16 11.801V4.697l-5.803 3.546Z" /></svg>
      : platform === "website" ? <Globe className="size-3.5" aria-hidden="true" /> : icon ? <svg viewBox="0 0 24 24" className="size-3.5" aria-hidden="true"><path fill="currentColor" d={icon.path} /></svg> : <span aria-hidden="true">{name.slice(0, 1)}</span>}
    </span>
    {!compact && <span className="min-w-0 text-sm [overflow-wrap:anywhere]">{name}</span>}
  </span>;
}

export function PlatformMark({ platform }: { platform: keyof typeof platforms }) {
  return <ChannelMark platform={platform} />;
}

/** Compatibility entry point: all channel icons share the same branded background. */
export function PlatformGlyph({ platform, className }: { platform: keyof typeof platforms; className?: string }) {
  return <span className={className}><ChannelMark platform={platform} compact /></span>;
}
