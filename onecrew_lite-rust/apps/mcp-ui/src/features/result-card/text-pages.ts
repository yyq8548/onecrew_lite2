export const TEXT_PAGE_CHARACTERS = 160;
export const TEXT_PAGE_LINES = 6;

// Bound wrapping and explicit newlines without dropping/normalizing content.
// Array.from keeps Unicode code points intact, including astral characters.
export function textPages(text: string): string[] {
  const pages: string[] = [];
  let page = "";
  let size = 0;
  let lines = 0;
  for (const character of Array.from(text)) {
    if (size === TEXT_PAGE_CHARACTERS || lines === TEXT_PAGE_LINES) {
      pages.push(page);
      page = ""; size = 0; lines = 0;
    }
    page += character;
    size += 1;
    if (character === "\n" || character === "\r") lines += 1;
  }
  if (page || !pages.length) pages.push(page);
  return pages;
}
