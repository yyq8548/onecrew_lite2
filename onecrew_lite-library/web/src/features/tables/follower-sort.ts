/** Sort saved counts numerically; keep estimates approximate and unknown values last. */
export function followerCount(value: string | null): number | null {
  if (!value) return null;
  const match = value.replace(/^(?:~|约)\s*/, '').replace(/,/g, '').replace(/\s*(?:位粉丝|粉丝|followers?)$/i, '').trim().match(/^(\d+(?:\.\d+)?)\s*([kmb万亿千]?)$/i);
  if (!match) return null;
  const scale: Record<string, number> = { k: 1e3, m: 1e6, b: 1e9, 千: 1e3, 万: 1e4, 亿: 1e8 };
  const count = Number(match[1]) * (scale[match[2].toLowerCase()] ?? 1);
  return Number.isFinite(count) ? count : null;
}

export function followerDigits(value: string | null): string | null {
  if (value === null) return null;
  const plain = value.trim();
  if (/^\d+$/.test(plain)) return plain;
  const count = followerCount(plain);
  return count !== null && Number.isSafeInteger(count) && count >= 0 ? String(count) : null;
}

export function sortByFollowers<T extends { accounts: Array<{ followers: string | null }> }>(rows: T[], ascending = false): T[] {
  // Multiple accounts remain separate counts: use the largest, never add audiences.
  const count = (row: T) => {
    const values = row.accounts.map(a => followerCount(a.followers)).filter((n): n is number => n !== null);
    return values.length ? Math.max(...values) : null;
  };
  return [...rows].sort((a, b) => {
    const x = count(a), y = count(b);
    if (x === null) return y === null ? 0 : 1;
    if (y === null) return -1;
    return ascending ? x - y : y - x;
  });
}
