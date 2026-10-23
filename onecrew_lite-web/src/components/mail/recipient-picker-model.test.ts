import { describe, expect, it } from "vitest";

import {
  RECIPIENT_SUGGESTION_LIMIT,
  creatorEmailChoices,
  looksLikeEmail,
  recipientCreatorCandidates,
  recipientSuggestionState,
} from "@/components/mail/recipient-picker-model";
import type { CreatorContactRouteRecord, CreatorRecord, CreatorSummary } from "@/lib/creator";

function summary(id: string, channels: CreatorSummary["channels"]): CreatorSummary {
  return {
    id,
    tags: [],
    display_name: `Creator ${id}`,
    stage: "prospect",
    platforms: [],
    channels,
    has_manual_origin: false,
    follower_counts: {},
    platform_account_count: 0,
    contact_route_count: channels.length,
    group: null,
    created_at: 1,
    updated_at: 1,
  };
}

function route(id: string, channel: "email" | "x", value: string): CreatorContactRouteRecord {
  return {
    route: { id, creator_id: "c1", locator_id: `loc-${id}`, confidence: "exact", created_at: 1, updated_at: 1 },
    locator: {
      id: `loc-${id}`,
      channel,
      kind: channel === "email" ? "email_address" : "handle",
      normalized_value: value,
      display_value: value,
      created_at: 1,
      updated_at: 1,
    },
    observations: [],
    allowed_actions: [],
  };
}

function record(routes: CreatorContactRouteRecord[]): CreatorRecord {
  return { contact_routes: routes } as unknown as CreatorRecord;
}

describe("recipient picker model", () => {
  describe("looksLikeEmail", () => {
    it("accepts a simple local@domain.tld shape, ignoring surrounding whitespace", () => {
      expect(looksLikeEmail("ada@example.com")).toBe(true);
      expect(looksLikeEmail("  ada.l+tag@sub.example.co  ")).toBe(true);
    });

    it("rejects names, handles, and incomplete addresses", () => {
      expect(looksLikeEmail("")).toBe(false);
      expect(looksLikeEmail("Ada Lovelace")).toBe(false);
      expect(looksLikeEmail("@ada")).toBe(false);
      expect(looksLikeEmail("ada@example")).toBe(false);
      expect(looksLikeEmail("ada@@example.com")).toBe(false);
      expect(looksLikeEmail("ada @example.com")).toBe(false);
    });
  });

  describe("recipientCreatorCandidates", () => {
    it("keeps only creators that have an email channel, preserving order", () => {
      const withEmail = summary("a", ["x", "email"]);
      const emailOnly = summary("b", ["email"]);
      const noEmail = summary("c", ["x", "instagram"]);
      expect(recipientCreatorCandidates([noEmail, withEmail, emailOnly])).toEqual([withEmail, emailOnly]);
      expect(recipientCreatorCandidates([])).toEqual([]);
    });
  });

  describe("creatorEmailChoices", () => {
    it("maps each email contact route to its route id and normalized address", () => {
      const detail = record([
        route("r1", "email", "ada@example.com"),
        route("r2", "x", "@ada"),
        route("r3", "email", "ada@work.example"),
      ]);
      expect(creatorEmailChoices(detail)).toEqual([
        { contactRouteId: "r1", email: "ada@example.com" },
        { contactRouteId: "r3", email: "ada@work.example" },
      ]);
    });

    it("returns nothing for a missing record or a creator without email routes", () => {
      expect(creatorEmailChoices(null)).toEqual([]);
      expect(creatorEmailChoices(undefined)).toEqual([]);
      expect(creatorEmailChoices(record([route("r2", "x", "@ada")]))).toEqual([]);
    });
  });

  describe("recipientSuggestionState", () => {
    const base = { query: "ada", debouncedQuery: "ada", loading: false, error: null, creators: undefined };

    it("is loading while the debounce lags behind the input or the query is in flight", () => {
      expect(recipientSuggestionState({ ...base, debouncedQuery: "ad" })).toEqual({ kind: "loading" });
      expect(recipientSuggestionState({ ...base, loading: true })).toEqual({ kind: "loading" });
    });

    it("surfaces a query error while leaving fixed fallback copy to the view", () => {
      expect(recipientSuggestionState({ ...base, error: new Error("boom") })).toEqual({ kind: "error", message: "boom" });
      expect(recipientSuggestionState({ ...base, error: {} })).toEqual({ kind: "error", message: null });
    });

    it("is empty when no candidate has an email channel", () => {
      expect(recipientSuggestionState({ ...base, creators: [] })).toEqual({ kind: "empty" });
      expect(recipientSuggestionState({ ...base, creators: [summary("c", ["x"])] })).toEqual({ kind: "empty" });
    });

    it("lists email-capable candidates capped at the suggestion limit", () => {
      const creators = Array.from({ length: RECIPIENT_SUGGESTION_LIMIT + 3 }, (_, index) => summary(`c${index}`, ["email"]));
      const state = recipientSuggestionState({ ...base, creators: [summary("x", ["x"]), ...creators] });
      expect(state.kind).toBe("ready");
      if (state.kind !== "ready") return;
      expect(state.creators).toHaveLength(RECIPIENT_SUGGESTION_LIMIT);
      expect(state.creators[0].id).toBe("c0");
    });
  });
});
