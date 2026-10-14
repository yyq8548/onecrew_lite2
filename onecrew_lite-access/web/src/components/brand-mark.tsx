import { cn } from "@/lib/utils";

export function BrandMark({ className }: { className?: string }) {
  return (
    <span className={cn("inline-flex items-center gap-2 font-semibold tracking-tight", className)}>
      <img className="h-8 w-auto" src="/onecrew-logo-pairing.png" alt="OneCrew" width="1480" height="322" />
    </span>
  );
}
