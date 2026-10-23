export function appendTags(values: string[], input: string, maxItems: number, maxLength: number, lengthOf: (text: string) => number = (text) => Array.from(text).length):
  { values: string[]; error: "count" | "length" | "duplicate" | null } {
  const additions = input.split(/[,\r\n]/).map((value) => value.trim()).filter(Boolean);
  const next = [...values];
  for (const value of additions) {
    if (lengthOf(value) > maxLength) return { values, error: "length" };
    if (!next.includes(value)) next.push(value);
  }
  if (additions.length && next.length == values.length) return { values, error: "duplicate" };
  if (next.length > maxItems) return { values, error: "count" };
  return { values: next, error: null };
}

export function tagCommitKey(key: string, composing: boolean, keyCode: number): boolean {
  return !composing && keyCode !== 229 && (key === "Enter" || key === ",");
}
