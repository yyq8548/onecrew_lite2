export type TimestampDateRange = {
  from: number;
  toExclusive: number;
};

function localDayStart(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function localDatesToTimestampRange(
  from: Date | undefined,
  to?: Date,
): TimestampDateRange | null {
  if (!from || !Number.isFinite(from.getTime()) || (to && !Number.isFinite(to.getTime()))) {
    return null;
  }
  const first = localDayStart(from);
  const last = localDayStart(to && to >= from ? to : from);
  const next = new Date(last.getFullYear(), last.getMonth(), last.getDate() + 1);
  return {
    from: Math.floor(first.getTime() / 1000),
    toExclusive: Math.floor(next.getTime() / 1000),
  };
}

export function readTimestampDateRange(value: unknown): TimestampDateRange | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<TimestampDateRange>;
  if (
    !Number.isInteger(candidate.from)
    || !Number.isInteger(candidate.toExclusive)
    || candidate.from! < 0
    || candidate.toExclusive! <= candidate.from!
  ) return null;
  return { from: candidate.from!, toExclusive: candidate.toExclusive! };
}

export function timestampMatchesDateRange(timestamp: unknown, value: unknown) {
  const range = readTimestampDateRange(value);
  if (!range) return true;
  return typeof timestamp === "number"
    && Number.isFinite(timestamp)
    && timestamp > 0
    && timestamp >= range.from
    && timestamp < range.toExclusive;
}

