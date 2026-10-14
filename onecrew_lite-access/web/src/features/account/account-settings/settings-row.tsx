import type { ReactNode } from "react";

import { FieldDescription, FieldLabel } from "@/components/ui/field";

export function SettingsRow({
  children,
  description,
  htmlFor,
  label,
}: {
  children: ReactNode;
  description?: ReactNode;
  htmlFor?: string;
  label: ReactNode;
}) {
  return (
    <div className="grid gap-3 border-b border-border px-6 py-5 md:grid-cols-[minmax(160px,0.7fr)_minmax(0,1.3fr)] md:items-center">
      <div className="grid gap-1">
        <FieldLabel htmlFor={htmlFor}>{label}</FieldLabel>
        {description ? <FieldDescription>{description}</FieldDescription> : null}
      </div>
      {children}
    </div>
  );
}
