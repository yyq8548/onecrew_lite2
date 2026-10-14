import { LoaderCircle } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";
import { cn } from "@/lib/utils";

export function AuthHeading({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="auth-heading">
      <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
      <p className="text-balance text-sm text-muted-foreground">{description}</p>
    </div>
  );
}

export function GoogleButton({
  disabled,
  enabled = true,
  loading,
  onClick,
}: {
  disabled: boolean;
  enabled?: boolean;
  loading: boolean;
  onClick: () => void;
}) {
  const { t } = useAccessI18n();
  return (
    <Button
      type="button"
      variant="outline"
      className="auth-google-button w-full"
      disabled={disabled}
      aria-busy={loading}
      onClick={onClick}
    >
      {loading ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : <GoogleGlyph />}
      {enabled ? t("auth.login.google") : t("auth.login.googleUnavailable")}
    </Button>
  );
}

export function FormMessage({ children }: { children: ReactNode }) {
  return (
    <div
      role="alert"
      aria-live="polite"
      className="rounded-md border border-destructive/25 bg-destructive/5 px-3 py-2 text-sm leading-5 text-destructive"
    >
      {children}
    </div>
  );
}

export function SubmitLabel({ pending, children }: { pending: boolean; children: ReactNode }) {
  return (
    <span className={cn("inline-flex items-center gap-2", pending && "opacity-90")}>
      {pending ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : null}
      {children}
    </span>
  );
}

function GoogleGlyph() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="size-4">
      <path fill="#4285F4" d="M21.6 12.23c0-.71-.06-1.4-.18-2.07H12v3.91h5.38a4.6 4.6 0 0 1-2 3.02v2.54h3.24c1.9-1.75 2.98-4.33 2.98-7.4Z" />
      <path fill="#34A853" d="M12 22c2.7 0 4.98-.9 6.64-2.43l-3.24-2.54c-.9.6-2.05.96-3.4.96-2.61 0-4.82-1.77-5.61-4.14H3.04v2.62A10 10 0 0 0 12 22Z" />
      <path fill="#FBBC05" d="M6.39 13.85A6 6 0 0 1 6.08 12c0-.64.11-1.27.31-1.85V7.53H3.04A10 10 0 0 0 2 12c0 1.61.39 3.14 1.04 4.47l3.35-2.62Z" />
      <path fill="#EA4335" d="M12 6.01c1.47 0 2.79.51 3.83 1.5l2.87-2.88A9.64 9.64 0 0 0 12 2 10 10 0 0 0 3.04 7.53l3.35 2.62C7.18 7.78 9.39 6.01 12 6.01Z" />
    </svg>
  );
}
