import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { BINDING } from "@mcp-ui/test-support/fixtures";

/** The exact shape the adapter's argument projection produces for a prepared
 * manual email: sorted tool argument keys and humanized labels. The approval
 * prompt is not repeated here — it belongs to the pending action alone, which
 * is what the App renders. The body carries real newline characters. */
export const MANUAL_EMAIL_BODY = "Hi River,\n\nI hope you're doing well! Just reaching out to say hello from the GoAI Partnerships team.\n\nBest regards,\nGoAI Partnerships";

export function manualEmailSnapshot(): ResultSnapshot {
  return {
    locale: "zh-CN",
    binding: structuredClone(BINDING),
    presented: true,
    completed_sections_count: 0,
    batch: {
      outcomes: [{
        call_id: "call.mail",
        operation_id: "email.manual.send",
        disposition: { kind: "not_run", reason: "confirmation_required" },
        summary: "Manual email requires confirmation.",
        resource_refs: [],
      }],
    },
    presentation: {
      kind: "simple_operation",
      title: "Send manual email",
      summary: "Confirm the prepared message before it is sent.",
      sections: [
        {
          kind: "fields",
          heading: "Review every submitted value",
          fields: [
            { label: "Confirmed", value: "true" },
            { label: "From", value: "堰离 (River) <river_fan@digifinex.org>" },
            { label: "Idempotency Key", value: "greeting-01a0a82b-3810-7122-a778-9e9928c5473b-river-20260916" },
            { label: "Mailbox Ref Id", value: "96e1a8ec-2242-41ad-8816-70263a22a5d3" },
            { label: "Subject", value: "Greetings from GoAI" },
            { label: "Text", value: MANUAL_EMAIL_BODY },
            { label: "To", value: "test@oncrewai.io" },
          ],
        },
      ],
    },
    // The label the adapter really sends: it is projected at mint, before the
    // card has a locale, so it arrives in the catalog's source language.
    pending_action: { kind: "confirm", prompt: "Send the prepared manual email to the explicitly selected address?", label: "Confirm" },
    state: { kind: "available" },
    continuation: { state: "not_requested", continuation_revision: 1, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
  };
}

/** The same card carrying a body that reached it already JSON-escaped, as the
 * adapter still does for a value no card can display. Whatever the reason, the
 * App renders the value it was given and never unescapes one. */
export function escapedManualEmailSnapshot(): ResultSnapshot {
  const snapshot = manualEmailSnapshot();
  const section = snapshot.presentation.sections[0]!;
  if (section.kind === "fields") {
    const body = section.fields.find((field) => field.label === "Text")!;
    body.value = JSON.stringify(MANUAL_EMAIL_BODY);
  }
  return snapshot;
}
