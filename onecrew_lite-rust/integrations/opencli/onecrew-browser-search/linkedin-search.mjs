// Read visible name links, not the encompassing card's hidden hiring label.
export function observeLinkedinPeople(query, limit) {
  const url = new URL(location.href);
  if (url.origin !== 'https://www.linkedin.com'
    || url.pathname !== '/search/results/people/' || url.searchParams.get('keywords') !== query)
    throw new Error('ONECREW_LINKEDIN_SEARCH_INVALID');
  const main = document.querySelector('main');
  if (!main) throw new Error('ONECREW_LINKEDIN_SEARCH_INVALID');
  const normalize = value => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  const rows = [], seen = new Set();
  for (const anchor of main.querySelectorAll('a[href*="/in/"]')) {
    if (!visible(anchor) || anchor.querySelector('a[href*="/in/"]')) continue;
    const profile = new URL(anchor.href);
    if (profile.origin !== url.origin || profile.username || profile.password || profile.port
      || !/^\/in\/[A-Za-z0-9_%\-]{1,200}\/$/.test(profile.pathname)) continue;
    const line = anchor.closest('p');
    // Search result name paragraphs include the native connection degree.
    if (!line || !/[•·]\s*(?:1st|2nd|3rd\+?)/.test(line.innerText)) continue;
    const name = normalize(anchor.innerText);
    if (!name || name.length > 200 || seen.has(profile.pathname)) continue;
    // Only read a visible headline from the same bounded native result card.
    // A whole-list ancestor could attribute another person's occupation to this link.
    const card = anchor.closest('li');
    const sameCard = card && [...card.querySelectorAll('a[href*="/in/"]')].every(link => {
      try { return new URL(link.href).pathname === profile.pathname; } catch { return false; }
    });
    const headlineNode = sameCard ? card.querySelector('.entity-result__primary-subtitle') : null;
    const headline = headlineNode && visible(headlineNode)
      ? Array.from(normalize(headlineNode.innerText)).slice(0, 300).join('') : '';
    seen.add(profile.pathname);
    rows.push({rank: rows.length + 1, name, headline, location: '',
      profile_url: `${url.origin}${profile.pathname}`});
    if (rows.length === limit) break;
  }
  if (!rows.length) throw new Error('ONECREW_LINKEDIN_SEARCH_INVALID');
  return rows;
}
