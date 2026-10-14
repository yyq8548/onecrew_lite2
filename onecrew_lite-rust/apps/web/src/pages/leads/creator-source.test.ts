import { describe, expect, it } from "vitest";

import type { CreatorOrigin, CreatorRecord } from "@/lib/creator";
import { creatorSourcePresentation } from "./creator-source";

const manualOrigin: CreatorOrigin = {
  kind: "manual_entry",
  id: "origin-manual",
  creator_id: "creator-1",
  source_id: "source-manual",
  created_at: 1,
};

const platformSearchOrigin: CreatorOrigin = {
  kind: "platform_search",
  id: "origin-platform-search",
  discovery_id: "discovery-1",
  account_ref_id: "account-1",
  creator_id: "creator-1",
  source_id: "source-platform",
  discovered_at: 1,
};

const importOrigin: CreatorOrigin = {
  kind: "creator_import",
  id: "origin-import",
  creator_id: "creator-1",
  source_id: "source-import",
  content_sha256: `sha256:${"01".repeat(32)}`,
  imported_creator_id: "portable-creator-1",
  imported_at: 1,
};

const detachedCommunityAdministratorOrigin: CreatorOrigin = {
  kind: "community_administrator",
  id: "origin-community-administrator",
  discovery_id: "discovery-community-administrator",
  creator_id: "creator-1",
  source_id: "source-community-administrator",
  role: "administrator",
  community: {
    external_id: "community-1",
    title: "Example Community",
    handle: "example-community",
    target_kind: "supergroup",
  },
  source: {
    kind: "creator_contact_route",
    source_creator_id: "source-creator",
    source_contact_route_id: null,
    operator_account_ref_id: "operator-1",
    source_resolved_target_id: "target-1",
  },
  discovered_at: 1,
};

function creatorRecord({
  origins = [],
  platform = false,
  archived = false,
}: {
  origins?: CreatorOrigin[];
  platform?: boolean;
  archived?: boolean;
}): CreatorRecord {
  return {
    creator: {
      id: "creator-1",
      display_name: "Example",
      tags: [],
      notes: null,
      follower_counts: {},
      created_at: 1,
      updated_at: 1,
      archived_at: archived ? 2 : null,
    },
    group: null,
    relationship: {
      creator_id: "creator-1",
      stage: archived ? "archived" : "prospect",
      last_touch_at: null,
      created_at: 1,
      updated_at: 1,
    },
    platform_accounts: platform ? [{
      id: "account-1",
      platform: "x",
      target_kind: "user",
      external_id: "123",
      created_at: 1,
      updated_at: 1,
    }] : [],
    account_links: [],
    account_observations: platform ? [{
      id: "account-observation-1",
      platform_account_id: "account-1",
      source_id: "source-platform",
      handle: "example",
      display_name: "Example",
      profile_url: "https://x.com/example",
      public_data: {},
      observed_at: 1,
    }] : [],
    observation_sources: [],
    origins,
    contact_routes: [],
  };
}

describe("Creator initial source presentation", () => {
  it("shows manual entry when no real platform account exists", () => {
    expect(creatorSourcePresentation(creatorRecord({ origins: [manualOrigin] }))).toEqual({
      kind: "manual_entry",
    });
  });

  it("prefers the real platform account for platform-search or mixed origins", () => {
    const source = creatorSourcePresentation(creatorRecord({
      origins: [manualOrigin, platformSearchOrigin],
      platform: true,
    }));
    expect(source.kind).toBe("platform");
    if (source.kind === "platform") {
      expect(source.accounts[0]).toMatchObject({ platform: "x", handle: "example", external_id: "123" });
    }
  });

  it("shows an em dash when neither platform nor origin exists", () => {
    expect(creatorSourcePresentation(creatorRecord({}))).toEqual({ kind: "none" });
  });

  it("shows file import provenance for an imported Creator without accounts", () => {
    expect(creatorSourcePresentation(creatorRecord({ origins: [importOrigin] }))).toEqual({
      kind: "creator_import",
    });
  });

  it("keeps a detached community-administrator source representable without fabricating an initial platform account", () => {
    expect(creatorSourcePresentation(creatorRecord({
      origins: [detachedCommunityAdministratorOrigin],
    }))).toEqual({ kind: "none" });
  });

  it("keeps the manual source visible after the Creator is archived", () => {
    expect(creatorSourcePresentation(creatorRecord({ origins: [manualOrigin], archived: true }))).toEqual({
      kind: "manual_entry",
    });
  });
});
