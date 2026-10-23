import type { CSSProperties } from "react"
import { createPortal } from "react-dom"
import { Toaster as Sonner, type ToasterProps } from "sonner"

export function Toaster({ ...props }: ToasterProps) {
  const content = (
    <Sonner
      className="toaster group [&_div[data-content]]:w-full"
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
        } as CSSProperties
      }
      {...props}
    />
  )

  // Sonner itself renders inline rather than through a portal. Keep this
  // host at body level so its fixed, high-z-index toaster is not trapped in
  // the AppShell's transformed stacking context (which contains Radix
  // overlays rendered into document.body).
  const body = typeof document === "undefined" ? null : document.body
  return body ? createPortal(content, body) : content
}
