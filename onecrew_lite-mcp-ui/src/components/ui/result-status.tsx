export type StatusTone = "success" | "failure" | "unknown" | "pending" | "neutral";
const statusPaths: Record<StatusTone, string> = {
  success: "M512 1024C229.248 1024 0 794.752 0 512S229.248 0 512 0s512 229.248 512 512-229.248 512-512 512z m-114.176-310.954667a53.333333 53.333333 0 0 0 75.434667 0l323.328-323.328a53.333333 53.333333 0 1 0-75.434667-75.434666l-287.914667 283.306666-128.853333-128.853333a53.333333 53.333333 0 1 0-75.434667 75.434667l168.874667 168.874666z",
  failure: "M511.8 63.3c-246.7 0-448.5 201.8-448.5 448.5s201.8 448.5 448.5 448.5 448.5-201.8 448.5-448.5S758.5 63.3 511.8 63.3z m214.3 603c15 15 15 42.4 0 57.3-7.5 7.5-17.4 12.5-29.9 12.5-10 0-22.4-5-29.9-12.5L521.8 579.1 377.3 723.6c-15 15-42.4 15-57.3 0s-15-42.4 0-57.3l144.5-144.5L320 377.3c-7.5-7.5-12.5-17.4-12.5-29.9 0-10 5-22.4 12.5-29.9 15-15 42.4-15 57.3 0L521.8 462l144.5-144.5c7.5-7.5 17.4-12.5 29.9-12.5 10 0 22.4 5 29.9 12.5s12.5 17.4 12.5 29.9c0 10-5 22.4-12.5 29.9L581.6 521.8l144.5 144.5z",
  unknown: "M512 0a512 512 0 1 0 0 1024A512 512 0 0 0 512 0Zm-48 240h96v352h-96Zm0 448h96v96h-96Z",
  pending: "M512 0a512 512 0 1 0 0 1024A512 512 0 0 0 512 0Zm-48 224h96v264l184 106-48 83-232-134Z",
  neutral: "M512 0a512 512 0 1 0 0 1024A512 512 0 0 0 512 0ZM256 464h512v96H256Z",
};
export function ResultStatus({ tone, label, iconOnly = false, busy = false }: { tone: StatusTone; label: string; iconOnly?: boolean; busy?: boolean }) {
  const color = tone === "success" ? "text-success-foreground" : tone === "failure" ? "text-destructive" : tone === "unknown" ? "text-warning-foreground" : "text-muted-foreground";
  return <span data-result-status={tone} role={iconOnly ? "img" : undefined} aria-label={iconOnly ? label : undefined} className={`inline-flex shrink-0 items-center gap-1.5 text-sm ${color}`}>
    {!iconOnly && <span>{label}</span>}
    <svg xmlns="http://www.w3.org/2000/svg" viewBox={tone === "failure" ? "63.3 63.3 897 897" : "0 0 1024 1024"} aria-hidden="true" focusable="false" className={`size-5 shrink-0 ${busy ? "animate-pulse motion-reduce:animate-none" : ""}`}>
      <path d={statusPaths[tone]} fill="currentColor" fillRule="evenodd" />
    </svg>
  </span>;
}
