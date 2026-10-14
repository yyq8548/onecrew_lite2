// YouTube utility extraction adapted from jackwener/OpenCLI.
// Source: clis/youtube/utils.js @ 8271afc67e8504bda94c147f446ee29775d08274 (Apache-2.0).
// See ../../../THIRD_PARTY_NOTICES.md and ../../licenses/APACHE-2.0.txt.
(function (root) {
  'use strict';

  /**
   * Extract a YouTube video ID from a URL or bare video ID string.
   * Supports: watch?v=, youtu.be/, /shorts/, /embed/, /live/, /v/
   */
  function parseVideoId(input) {
    if (!input.startsWith('http')) return input;
    try {
      const parsed = new URL(input);
      if (parsed.searchParams.has('v')) {
        return parsed.searchParams.get('v');
      }
      if (parsed.hostname === 'youtu.be') {
        return parsed.pathname.slice(1).split('/')[0];
      }
      // Handle /shorts/xxx, /embed/xxx, /live/xxx, /v/xxx
      const pathMatch = parsed.pathname.match(/^\/(shorts|embed|live|v)\/([^/?]+)/);
      if (pathMatch) return pathMatch[2];
    } catch {
      // Not a valid URL — treat entire input as video ID
    }
    return input;
  }

  /**
   * Extract a JSON object assigned to a known bootstrap variable inside YouTube HTML.
   */
  function extractJsonAssignmentFromHtml(html, keys) {
    const candidates = Array.isArray(keys) ? keys : [keys];
    for (const key of candidates) {
      // prettier-ignore
      const markers = [
        `var ${key} = `,
        `window["${key}"] = `,
        `window.${key} = `,
        `${key} = `,
      ];
      for (const marker of markers) {
        const markerIndex = html.indexOf(marker);
        if (markerIndex === -1) continue;
        const jsonStart = html.indexOf('{', markerIndex + marker.length);
        if (jsonStart === -1) continue;
        let depth = 0;
        let inString = false;
        let escaping = false;
        for (let i = jsonStart; i < html.length; i += 1) {
          const ch = html[i];
          if (inString) {
            if (escaping) {
              escaping = false;
            } else if (ch === '\\') {
              escaping = true;
            } else if (ch === '"') {
              inString = false;
            }
            continue;
          }
          if (ch === '"') {
            inString = true;
            continue;
          }
          if (ch === '{') {
            depth += 1;
            continue;
          }
          if (ch === '}') {
            depth -= 1;
            if (depth === 0) {
              try {
                return JSON.parse(html.slice(jsonStart, i + 1));
              } catch {
                break;
              }
            }
          }
        }
      }
    }
    return null;
  }

  /**
   * SAPISIDHASH helper for authenticated YouTube InnerTube requests.
   */
  async function getSapisidHash(sapisid, origin) {
    if (!sapisid) return null;
    const time = Math.floor(Date.now() / 1000);
    const msgBuffer = new TextEncoder().encode(time + ' ' + sapisid + ' ' + origin);
    const hashBuffer = await crypto.subtle.digest('SHA-1', msgBuffer);
    const hashHex = Array.from(new Uint8Array(hashBuffer))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
    return 'SAPISIDHASH ' + time + '_' + hashHex;
  }

  root.OneCrewYoutubeReaders = root.OneCrewYoutubeReaders || {};
  Object.assign(root.OneCrewYoutubeReaders, {
    extractJsonAssignmentFromHtml,
    getSapisidHash,
    parseVideoId,
  });
})(globalThis);
