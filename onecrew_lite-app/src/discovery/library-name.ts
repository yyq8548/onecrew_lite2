export function discoveryProductName(brief: string): string {
  const first = (brief.split(/[，,。;；\n]/u)[0] ?? "").trim();
  let product = first;
  for (const prefix of ["我要推广", "我想推广", "我要卖", "我想卖", "我在卖", "我卖", "推广", "卖"]) if (first.startsWith(prefix)) { product = first.slice(prefix.length).trim(); break; }
  const lower = product.replace(/[A-Z]/gu, value => value.toLowerCase());
  for (const prefix of ["i want to sell ", "i am selling ", "i'm selling ", "i sell ", "selling ", "promote "]) if (lower.startsWith(prefix)) { product = product.slice(prefix.length).trim(); break; }
  for (const marker of ["，", "想找", "希望找", "帮我找", "找达人", "寻找达人"]) { const index = product.indexOf(marker); if (index > 0) product = product.slice(0, index).trim(); }
  return [...product].filter(character => !/\p{Cc}/u.test(character)).slice(0, 120).join("");
}
