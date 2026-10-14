export type EdgeLocation = {
  country: string | null;
  region: string | null;
  city: string | null;
  colo: string | null;
};

type RequestCf = {
  country?: unknown;
  region?: unknown;
  city?: unknown;
  colo?: unknown;
};

function text(value: unknown, maxLength: number) {
  if (typeof value !== "string") return null;
  const normalized = value.trim();
  return normalized.length > 0 && normalized.length <= maxLength ? normalized : null;
}

export function edgeLocation(request: Request): EdgeLocation {
  const cf = (request as Request & { cf?: RequestCf }).cf;
  return {
    country: text(cf?.country ?? request.headers.get("CF-IPCountry"), 8),
    region: text(cf?.region, 128),
    city: text(cf?.city, 128),
    colo: text(cf?.colo, 8),
  };
}

export function edgeLocationFromRow(row: {
  pairing_country?: string | null;
  pairing_region?: string | null;
  pairing_city?: string | null;
  pairing_colo?: string | null;
}): EdgeLocation {
  return {
    country: row.pairing_country ?? null,
    region: row.pairing_region ?? null,
    city: row.pairing_city ?? null,
    colo: row.pairing_colo ?? null,
  };
}

export function hasEdgeLocation(location: EdgeLocation) {
  return Object.values(location).some((value) => value !== null);
}
