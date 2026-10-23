import { describe, expect, it, vi } from "vitest";

import type {
  ContactChannel,
  CreatorAccountView,
  CreatorContactRouteAction,
  CreatorContactRouteRecord,
} from "@/lib/creator";
import { routeExternalActionUrl, savedContactRouteRows } from "./contact-routes";

function urlRoute(
  index: number,
  channel: ContactChannel,
  value: string,
  allowedActions: CreatorContactRouteAction[] = [],
): CreatorContactRouteRecord {
  return {
    route: {
      id: `route-${index}`,
      creator_id: "creator-1",
      locator_id: `locator-${index}`,
      confidence: "asserted",
      created_at: 1,
      updated_at: 1,
    },
    locator: {
      id: `locator-${index}`,
      channel,
      kind: "url",
      normalized_value: value,
      display_value: value,
      created_at: 1,
      updated_at: 1,
    },
    observations: [],
    allowed_actions: allowedActions,
  };
}

describe("saved contact route actions", () => {
  it("preserves TikTok URL locators and authoritative account profile URLs", () => {
    const direct = urlRoute(20, "tiktok", "https://www.tiktok.com/@url.creator");
    const noHandle: CreatorAccountView = {
      platform: "tiktok", target_kind: "creator", external_id: "tto:provider-id", handle: null,
      display_name: "Creator", profile_url: "https://www.tiktok.com/@profile.creator", public_data: {}, origin: "initial_platform_account",
    };
    const rows = savedContactRouteRows([direct], [noHandle], false);
    expect(rows.map(routeExternalActionUrl)).toEqual([
      "https://www.tiktok.com/@profile.creator", "https://www.tiktok.com/@url.creator",
    ]);
    const [withoutProfile] = savedContactRouteRows([], [{ ...noHandle, external_id: "12345678", profile_url: null }], false);
    expect(routeExternalActionUrl(withoutProfile!)).toBeNull();
  });

  it("constructs TikTok destinations only from typed handles, leaving invalid display text unguessed", () => {
    const handle = urlRoute(21, "tiktok", "@real.creator");
    handle.locator.kind = "handle";
    const invalid = urlRoute(22, "tiktok", "https://example.com/@other");
    const rows = savedContactRouteRows([handle, invalid], [], false);
    expect(rows.map(routeExternalActionUrl)).toEqual(["https://www.tiktok.com/@real.creator", null]);
  });

  const routes = [
    urlRoute(1, "website", "https://example.com"),
    urlRoute(2, "link_hub", "https://linktr.ee/example"),
    urlRoute(3, "youtube", "https://youtube.com/@example"),
  ];
  const accounts: CreatorAccountView[] = [{
    platform: "x",
    target_kind: "user",
    external_id: "123",
    handle: "example",
    display_name: "Example",
    profile_url: "https://x.com/example",
    public_data: {},
      origin: "initial_platform_account",
  }];

  it("composes archived website, link hub, YouTube and account routes without external actions", () => {
    const rows = savedContactRouteRows(routes, accounts, true);
    expect(rows).toHaveLength(4);
    for (const row of rows) {
      expect(routeExternalActionUrl(row)).toBeNull();
    }
  });

  it("keeps the composed external actions for an active Creator", () => {
    const rows = savedContactRouteRows(routes, accounts, false);
    expect(rows.map(routeExternalActionUrl)).toEqual([
      "https://x.com/example",
      "https://example.com",
      "https://linktr.ee/example",
      "https://youtube.com/@example",
    ]);
  });

  it("composes route menus only from runtime allowed_actions and never for synthetic account rows", () => {
    const editable = urlRoute(4, "website", "https://editable.example", ["update", "delete"]);
    const deleteOnly = urlRoute(5, "website", "https://delete.example", ["delete"]);
    const onEdit = vi.fn();
    const onDelete = vi.fn();
    const rows = savedContactRouteRows([editable, deleteOnly], accounts, false, onEdit, onDelete);

    expect(rows[0].key).toMatch(/^account:/);
    expect(rows[0].onEdit).toBeUndefined();
    expect(rows[0].onDelete).toBeUndefined();
    expect(rows[1].onEdit).toBeTypeOf("function");
    expect(rows[1].onDelete).toBeTypeOf("function");
    expect(rows[2].onEdit).toBeUndefined();
    expect(rows[2].onDelete).toBeTypeOf("function");

    rows[1].onEdit?.();
    rows[2].onDelete?.();
    expect(onEdit).toHaveBeenCalledWith(editable);
    expect(onDelete).toHaveBeenCalledWith(deleteOnly);
  });

  it("does not infer mutation policy from initial-platform provenance", () => {
    const derivedInitial = urlRoute(7, "website", "https://derived.example", ["delete"]);
    derivedInitial.observations = [{
      observation: {
        id: "observation-7",
        locator_id: "locator-7",
        source_id: "source-7",
        creator_id: "creator-1",
        observed_value: "https://derived.example",
        observed_at: 1,
      },
      source: {
        id: "source-7",
        kind: "initial_platform_account",
        platform: "x",
        source_url: "https://x.com/example",
        metadata: {},
        observed_at: 1,
      },
    }];
    const onDelete = vi.fn();
    const [row] = savedContactRouteRows([derivedInitial], [], false, undefined, onDelete);

    expect(row.onDelete).toBeTypeOf("function");
    row.onDelete?.();
    expect(onDelete).toHaveBeenCalledWith(derivedInitial);
  });

  it("removes all route mutation actions in read-only details", () => {
    const editable = urlRoute(6, "website", "https://editable.example", ["update", "delete"]);
    const rows = savedContactRouteRows([editable], accounts, true, vi.fn(), vi.fn());
    expect(rows.every((row) => !row.onEdit && !row.onDelete)).toBe(true);
  });
});
