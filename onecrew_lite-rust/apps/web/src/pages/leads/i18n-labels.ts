import type { TFunction } from "i18next";

import type { ContactChannel, CreatorStage, SourceKind } from "@/lib/creator";

export type LeadsT = TFunction<"leads">;

const STAGE_KEYS = {
  prospect: "stages.prospect",
  engaged: "stages.engaged",
  collaborating: "stages.collaborating",
  archived: "stages.archived",
} as const satisfies Record<CreatorStage, string>;

const CHANNEL_KEYS = {
  email: "channels.email",
  wechat: "channels.wechat",
  x: "channels.x",
  instagram: "channels.instagram",
  telegram: "channels.telegram",
  youtube: "channels.youtube",
  tiktok: "channels.tiktok",
  website: "channels.website",
  link_hub: "channels.linkHub",
} as const satisfies Record<ContactChannel, string>;

const SOURCE_KEYS = {
  initial_platform_account: "sources.initialPlatformAccount",
  profile_bio: "sources.profileBio",
  profile_website: "sources.profileWebsite",
  profile_location: "sources.profileLocation",
  video_description: "sources.videoDescription",
  channel_description: "sources.channelDescription",
  link_hub: "sources.linkHub",
  manual_entry: "sources.manualEntry",
  manual_business_email: "sources.manualBusinessEmail",
  community_administrator: "sources.communityAdministrator",
  contact_route_resolution: "sources.contactRouteResolution",
  creator_import: "sources.creatorImport",
} as const satisfies Record<SourceKind, string>;

export function creatorStageLabel(t: LeadsT, stage: CreatorStage) {
  return t(STAGE_KEYS[stage]);
}

export function contactChannelLabel(t: LeadsT, channel: string) {
  const key = CHANNEL_KEYS[channel as ContactChannel];
  return key ? t(key) : channel;
}

export function sourceKindLabel(t: LeadsT, source: SourceKind) {
  return t(SOURCE_KEYS[source]);
}
