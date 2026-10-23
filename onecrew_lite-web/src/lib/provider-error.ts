import { appI18n } from "@/i18n";
import zhCNCommon from "@/i18n/locales/zh-CN/common";

/** Wire evidence from the actual protocol. Equal codes need not mean equal causes. */
export type DmDiagnostic = {
  platform: string;
  protocol: string;
  origin: "provider" | "local";
  code: string | null;
  http_status: number | null;
  rpc_code: number | null;
  message: string;
};
export type DmFailure = { stage: string; diagnostic: DmDiagnostic };
export type OperationFailure = { code: string; message: string; failure?: DmFailure };
export type ProviderErrorPresentation = {
  summary: string;
  details: ReadonlyArray<{ label: string; value: string }>;
};

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function nonempty(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}
function nullableInteger(value: unknown, min: number, max: number): value is number | null {
  return value === null || (typeof value === "number" && Number.isInteger(value) && value >= min && value <= max);
}

/** Validate the current response shape without interpreting platform values. */
export function readDmFailure(value: unknown): DmFailure | null {
  if (!record(value) || !nonempty(value.stage) || !record(value.diagnostic)) return null;
  const diagnostic = value.diagnostic;
  if (!nonempty(diagnostic.platform) || !nonempty(diagnostic.protocol)
    || (diagnostic.origin !== "provider" && diagnostic.origin !== "local")
    || (diagnostic.code !== null && typeof diagnostic.code !== "string")
    || !nullableInteger(diagnostic.http_status, 0, 65_535)
    || !nullableInteger(diagnostic.rpc_code, -2_147_483_648, 2_147_483_647)
    || !nonempty(diagnostic.message)) return null;
  return { stage: value.stage, diagnostic: {
    platform: diagnostic.platform, protocol: diagnostic.protocol, origin: diagnostic.origin,
    code: diagnostic.code, http_status: diagnostic.http_status, rpc_code: diagnostic.rpc_code,
    message: diagnostic.message,
  } };
}

function readOperationFailure(value: unknown): OperationFailure | null {
  if (!record(value) || !nonempty(value.code) || !nonempty(value.message)) return null;
  if (value.failure === undefined) return { code: value.code, message: value.message };
  const failure = readDmFailure(value.failure);
  return failure ? { code: value.code, message: value.message, failure } : null;
}

type Label = keyof typeof zhCNCommon.providerError.labels;
function label(key: Label): string {
  return appI18n.isInitialized
    ? appI18n.t(`providerError.labels.${key}`, { ns: "common" })
    : zhCNCommon.providerError.labels[key];
}
function unavailable(): ProviderErrorPresentation {
  return { summary: appI18n.isInitialized
    ? appI18n.t("providerError.unavailable", { ns: "common" })
    : zhCNCommon.providerError.unavailable, details: [] };
}

function nativePresentation(failure: DmFailure): ProviderErrorPresentation {
  const diagnostic = failure.diagnostic;
  const origin = appI18n.isInitialized
    ? appI18n.t(`providerError.origins.${diagnostic.origin}`, { ns: "common" })
    : zhCNCommon.providerError.origins[diagnostic.origin];
  const details = [
    { label: label("stage"), value: failure.stage },
    { label: label("platform"), value: diagnostic.platform },
    { label: label("protocol"), value: diagnostic.protocol },
    { label: label("origin"), value: origin },
  ];
  details.push({ label: label(diagnostic.origin === "provider" ? "nativeCode" : "localCode"),
    value: diagnostic.code ?? (appI18n.isInitialized
      ? appI18n.t("providerError.notProvided", { ns: "common" }) : zhCNCommon.providerError.notProvided),
  });
  if (diagnostic.http_status !== null) details.push({ label: label("httpStatus"), value: String(diagnostic.http_status) });
  if (diagnostic.rpc_code !== null) details.push({ label: label("rpcCode"), value: String(diagnostic.rpc_code) });
  return { summary: diagnostic.message, details };
}

/** A failed HTTP operation supplies a safe owner envelope, never a raw cause. */
export function presentProviderError(error: unknown): ProviderErrorPresentation {
  const failure = readOperationFailure(error);
  if (!failure) return unavailable();
  const presentation = failure.failure
    ? nativePresentation(failure.failure)
    : { summary: failure.message, details: [] };
  return { ...presentation, details: [{ label: label("ownerCode"), value: failure.code }, ...presentation.details] };
}

/** A completion may contain a readback diagnostic without being a failed send. */
export function presentDmFailure(value: unknown): ProviderErrorPresentation {
  const failure = readDmFailure(value);
  return failure ? nativePresentation(failure) : unavailable();
}

/** Human presentation only; callers must use owner state for every action. */
export function diagnosticText(presentation: ProviderErrorPresentation): string {
  return [presentation.summary, ...presentation.details.map(({ label, value }) => `${label}: ${value}`)].join("\n");
}
