import type { TFunction } from "i18next";

import { isEmailTemplatePaginationLimitError } from "@/components/mail/queries";

export function emailTemplateLoadErrorDescription(
  error: unknown,
  t: TFunction<"mail">,
): string {
  if (isEmailTemplatePaginationLimitError(error)) {
    return t("templates.errors.paginationLimit");
  }
  return String(error);
}
