import { useId, useState } from "react";
import { useAppLocale } from "@mcp-ui/lib/i18n";

export function ExpandableText({ text, maxCharacters = 160, maxLines = 4 }: {
  text: string; maxCharacters?: number; maxLines?: number;
}) {
  const { t } = useAppLocale();
  const [expanded, setExpanded] = useState(false);
  const id = useId();
  const expandable = Array.from(text).length > maxCharacters || text.split("\n").length > maxLines;
  return <div className="min-w-0 text-sm">
    <p id={id} className={`whitespace-pre-wrap break-words leading-5 ${expandable && !expanded ? "line-clamp-2" : ""}`}>{text}</p>
    {expandable && <button type="button" className="mt-1 cursor-pointer text-xs text-muted-foreground hover:underline" aria-expanded={expanded} aria-controls={id} onClick={() => setExpanded(value => !value)}>{t(expanded ? "collapseText" : "details")}</button>}
  </div>;
}
