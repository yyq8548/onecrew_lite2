import { cn } from "@/lib/utils";

export function OneCrewMark({ className }: { className?: string }) {
  return (
    <img
      src="/onecrew-ai-mark.png"
      className={cn("rounded-[22%] object-contain", className)}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}
