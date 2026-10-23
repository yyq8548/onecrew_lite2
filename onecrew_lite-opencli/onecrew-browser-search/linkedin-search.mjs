// Read visible name links, not the encompassing card's hidden hiring label.
export async function readLinkedinPeopleWhenReady(query, limit, observe, readStop) {
  let previous = '';
  for (let poll = 0; poll < 24; poll++) {
    const stop = readStop();
    if (stop === 'auth' || stop === 'risk') throw new Error('ONECREW_LINKEDIN_SEARCH_'+stop.toUpperCase());
    const main = document.querySelector('main');
    const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
    const links = main ? [...main.querySelectorAll('a[href*="/in/"]')].filter(node => visible(node) && /[•·]\s*(?:1st|2nd|3rd\+?)/.test(node.closest('p')?.innerText ?? '')) : [];
    const signature = links.map(node => node.href).join('\n');
    const loading = main && [...main.querySelectorAll('[role="progressbar"],[aria-busy="true"]')].some(visible);
    if (links.length >= limit || signature && signature === previous && !loading) return observe(query, limit);
    previous = signature;
    if (poll < 23) await new Promise(resolve => setTimeout(resolve, 250));
  }
  return observe(query, limit);
}

export function observeLinkedinPeople(query, limit) {
  const url = new URL(location.href);
  if (url.origin !== 'https://www.linkedin.com'
    || url.pathname !== '/search/results/people/' || url.searchParams.get('keywords') !== query)
    throw new Error(`ONECREW_LINKEDIN_SEARCH_INVALID stage=page_binding origin_matches=${url.origin === 'https://www.linkedin.com'} path_matches=${url.pathname === '/search/results/people/'} query_matches=${url.searchParams.get('keywords') === query}`);
  const main = document.querySelector('main');
  if (!main) throw new Error('ONECREW_LINKEDIN_SEARCH_INVALID stage=results_container');
  const normalize = value => String(value || '').replace(/\s+/g, ' ').trim();
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  const rows = [], seen = new Set();
  const anchors = main.querySelectorAll('a[href*="/in/"]');
  let visibleProfiles = 0, connectionLines = 0;
  for (const anchor of anchors) {
    if (!visible(anchor) || anchor.querySelector('a[href*="/in/"]')) continue;
    const profile = new URL(anchor.href);
    if (profile.origin !== url.origin || profile.username || profile.password || profile.port
      || !/^\/in\/[A-Za-z0-9_%\-]{1,200}\/$/.test(profile.pathname)) continue;
    visibleProfiles++;
    const line = anchor.closest('p');
    // Search result name paragraphs include the native connection degree.
    if (!line || !/[•·]\s*(?:1st|2nd|3rd\+?)/.test(line.innerText)) continue;
    connectionLines++;
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
  if (!rows.length) throw new Error(`ONECREW_LINKEDIN_SEARCH_INVALID stage=result_cards profile_links=${anchors.length} visible_profiles=${visibleProfiles} connection_lines=${connectionLines}`);
  return rows;
}
