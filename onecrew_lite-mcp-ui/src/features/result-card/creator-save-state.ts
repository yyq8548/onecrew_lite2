import type { CreatorSavePreview, CreatorSaveRoute } from "@mcp-ui/lib/result-contract";

export const creatorAlreadySaved = (preview: CreatorSavePreview) => preview.status === "existing_skipped";

export const creatorSaveRowSelectable = (preview: CreatorSavePreview) =>
  preview.status === "eligible_new" || preview.status === "eligible_update";

export const selectableCreatorRoutes = (preview: CreatorSavePreview) =>
  creatorSaveRowSelectable(preview)
    ? preview.routes.filter(route => route.collision.status === "none")
    : [];

/** The only channel that does not name an account or address by itself. */
const GENERIC_CHANNEL = "website";
/** Sources that are a mention inside prose rather than something the creator
 * curated about themselves. */
const PROSE_SOURCES = new Set(["video_description", "channel_description"]);

/** The optional routes a review card checks for the user.
 *
 * A creator's link block is inherited by every upload, so their descriptions
 * hand back sponsor, affiliate, merch and forum links alongside the two
 * contacts that matter — one real creator arrived with fifteen routes, every
 * one of them pre-checked. Check what reaches them: an address, a named
 * account, or a site they curated. Withhold the check only on positive
 * evidence — a bare URL whose every source is prose — and never on a route
 * whose provenance we do not know. Nothing is dropped either way: the route
 * stays listed and one click away, because saving a route has never been a
 * claim that it is reachable. */
export function defaultCreatorRouteIds(preview: CreatorSavePreview): string[] {
  return selectableCreatorRoutes(preview)
    .filter((route) => !route.required)
    .filter((route) => route.channel !== GENERIC_CHANNEL
      || route.sources.length === 0
      || route.sources.some((source) => !PROSE_SOURCES.has(source)))
    .map((route) => route.route_id);
}
export const creatorSaveBlocked = (preview: CreatorSavePreview) => !creatorSaveRowSelectable(preview);

export function creatorSaveRouteIds(preview: CreatorSavePreview, optionalIds: string[]): string[] {
  return selectableCreatorRoutes(preview).filter((route) => route.required || optionalIds.includes(route.route_id)).map((route) => route.route_id);
}

export function toggleCreatorSaveRoute(optionalIds: string[], route: CreatorSaveRoute, checked: boolean): string[] {
  if (route.collision.status !== "none") return optionalIds.filter(id => id !== route.route_id);
  if (route.required) return optionalIds;
  return checked ? [...new Set([...optionalIds, route.route_id])] : optionalIds.filter((id) => id !== route.route_id);
}
