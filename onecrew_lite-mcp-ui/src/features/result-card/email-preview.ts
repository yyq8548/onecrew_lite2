import type { PresentationProjection, PresentationSection } from "@mcp-ui/lib/result-contract";

/** One sender or recipient, split only when the wire already spelled it that way. */
export type EmailParty = { name: string | null; address: string | null };

/** A projected message, derived once so the view renders nothing but state.
 *
 * `body` is the exact wire value. This layer never unescapes it: a projection
 * that already carries the characters backslash and n was escaped upstream, and
 * inventing a newline here would claim content the card was never given. */
export type EmailPreviewModel = {
  from: EmailParty | null;
  to: string[];
  subject: string | null;
  date: string | null;
  body: string;
  details: EmailDetail[];
};

export type EmailDetail = { key: EmailDetailKey; label: string | null; value: string };
export type EmailDetailKey = "from" | "to" | "date" | "subject" | "other";

// A closed label vocabulary from the two projection owners that can produce a
// message review: the adapter's exact argument projection (English keys derived
// from the tool schema) and the runtime's localized outreach review. Direct
// message bodies ("Message" / "私信正文") stay out: they are their own review.
const FROM = ["from", "sender", "from address", "发件人"];
const TO = ["to", "recipient", "recipients", "to address", "收件人"];
const SUBJECT = ["subject", "邮件主题", "主题"];
const BODY = ["text", "body", "email body", "邮件正文", "正文"];
const DATE = ["date", "sent", "sent at", "发送时间", "日期"];

/** Compare a projected label without its array index or spacing noise. */
export function emailFieldName(label: string): string {
  return label.replace(/\[\d+\]/gu, "").replace(/\s+/gu, " ").trim().toLowerCase();
}

/** Split `Name <address>` when the value already carries both halves. */
export function emailParty(value: string): EmailParty {
  const bracketed = /^\s*(.*?)\s*<([^<>\s]+)>\s*$/u.exec(value);
  if (bracketed) {
    const name = bracketed[1]!.replace(/^["']|["']$/gu, "").trim();
    return { name: name.length ? name : null, address: bracketed[2]! };
  }
  const trimmed = value.trim();
  return /^[^\s<>@]+@[^\s<>@]+$/u.test(trimmed) ? { name: null, address: trimmed } : { name: trimmed, address: null };
}

/** The initial shown in the avatar, safe for astral characters. */
export function emailInitial(party: EmailParty | null): string {
  const source = party?.name ?? party?.address ?? "";
  return Array.from(source)[0]?.toUpperCase() ?? "?";
}

/** Recognize a projected message, or `null` for every other projection.
 *
 * A projection qualifies only when it names both a recipient and a body, so an
 * ordinary field table keeps the table it already had. */
export function emailPreview(projection: PresentationProjection): EmailPreviewModel | null {
  const fields = projection.sections.flatMap((section) =>
    section.kind === "fields" ? section.fields : []);
  if (!fields.length) return null;

  let from: EmailParty | null = null;
  let subject: string | null = null;
  let date: string | null = null;
  let body: string | null = null;
  const to: string[] = [];
  const rest: EmailDetail[] = [];

  for (const field of fields) {
    const name = emailFieldName(field.label);
    if (FROM.includes(name) && from == null) from = emailParty(field.value);
    else if (TO.includes(name)) to.push(field.value);
    else if (SUBJECT.includes(name) && subject == null) subject = field.value;
    else if (DATE.includes(name) && date == null) date = field.value;
    else if (BODY.includes(name) && body == null) body = field.value;
    else rest.push({ key: "other", label: field.label, value: field.value });
  }
  if (!to.length || body == null) return null;

  return {
    from,
    to,
    subject,
    date,
    body,
    // Every projected value stays reachable: the four message facts lead, and
    // no remaining field is dropped just because the preview does not show it.
    details: [
      ...(from ? [{ key: "from" as const, label: null, value: emailPartyText(from) }] : []),
      ...to.map((value) => ({ key: "to" as const, label: null, value })),
      ...(date ? [{ key: "date" as const, label: null, value: date }] : []),
      ...(subject ? [{ key: "subject" as const, label: null, value: subject }] : []),
      ...rest,
    ],
  };
}

/** Rejoin a party for a single-line detail row without losing either half. */
export function emailPartyText(party: EmailParty): string {
  if (party.name && party.address) return `${party.name} <${party.address}>`;
  return party.name ?? party.address ?? "";
}

/** Sections a message preview does not own; the card renders them unchanged. */
export function emailAsideSections(projection: PresentationProjection): PresentationSection[] {
  return projection.sections.filter((section) => section.kind !== "fields");
}
