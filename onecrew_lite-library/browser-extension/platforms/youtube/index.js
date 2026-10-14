// YouTube workflow orchestration adapted from jackwener/OpenCLI (Apache-2.0).
// OneCrew changes provide the fixed video -> channel KOL discovery flow and bridge lifecycle.
// See ../../../THIRD_PARTY_NOTICES.md and ../../licenses/APACHE-2.0.txt.
importScripts(
  'platforms/youtube/utils.js',
  'platforms/youtube/search.js',
  'platforms/youtube/video.js',
  'platforms/youtube/channel.js',
);

(function (root) {
  'use strict';
  const READINESS_ERROR_MARKER = 'ONECREW_YOUTUBE_NOT_READY';
  const READER_FILES = [
    'platforms/youtube/utils.js',
    'platforms/youtube/search.js',
    'platforms/youtube/video.js',
    'platforms/youtube/channel.js',
  ];

  function messages() {
    const get = (key) => (root.chrome.i18n && root.chrome.i18n.getMessage(key)) || key;
    return {
      consentRequired: get('youtubeConsentRequired'),
      notReady: get('youtubeNotReady'),
      paginationTimeout: get('youtubePaginationTimeout'),
      repeatedCursor: get('youtubeRepeatedCursor'),
      signedInCookie: get('youtubeSignedInCookie'),
    };
  }
  async function cookies() {
    const values = {};
    for (const name of ['__Secure-3PAPISID', 'SAPISID']) {
      const cookie = await root.chrome.cookies.get({ url: 'https://www.youtube.com/', name });
      if (cookie?.value) {
        values[name] = cookie.value;
        if (!values.SAPISID) values.SAPISID = cookie.value;
      }
    }
    return values;
  }
  async function execute(tabId, func, args, files = []) {
    const tab = await root.chrome.tabs.get(tabId);
    if (!root.OneCrewYoutubePlatform.supportsUrl(tab.url))
      throw new Error(root.chrome.i18n.getMessage('youtubeUnsupportedYoutubePage'));
    if (files.length)
      await root.chrome.scripting.executeScript({
        target: { tabId },
        world: 'MAIN',
        files,
      });
    const result = await root.chrome.scripting.executeScript({
      target: { tabId },
      world: 'MAIN',
      func,
      args: args || [],
    });
    return result[0] && result[0].result;
  }

  function youtubeDiscoverKols(_query, limit, signedCookies, localizedMessages = {}) {
    return (async () => {
      const readinessErrorMarker = 'ONECREW_YOUTUBE_NOT_READY';
      const readers = globalThis.OneCrewYoutubeReaders;
      const localized = (key, fallback) => localizedMessages[key] || fallback;
      const config = globalThis.ytcfg?.data_ || {};
      if (document.querySelector('ytd-consent-bump-v2, ytd-captcha'))
        throw new Error(
          localized(
            'consentRequired',
            'YouTube requires consent or captcha before search results can be read safely',
          ),
        );
      if (!config.INNERTUBE_API_KEY || !config.INNERTUBE_CONTEXT || !globalThis.ytInitialData)
        throw new Error(
          readinessErrorMarker +
            ': ' +
            localized('notReady', 'YouTube page is not ready for search data'),
        );
      if (!readers?.searchPage || !readers.getVideo || !readers.getChannel)
        throw new Error(readinessErrorMarker + ': YouTube reader scripts are not ready');

      const maxPages = 10;
      const maxVideos = 200;
      const deadline = Date.now() + 180000;
      const sapisid = signedCookies?.['__Secure-3PAPISID'] || signedCookies?.SAPISID || null;
      const warningList = [];
      const seenVideos = new Set();
      const seenCursors = new Set();
      const channels = new Map();
      const matched = new Map();
      const attemptedChannels = new Set();
      const completed = [];

      const timeoutMs = () => Math.min(15000, Math.max(1, deadline - Date.now()));
      const fatal = (value) => {
        const message = String(value?.message || value?.error || value || '');
        if (
          /auth|unauth|captcha|consent|login|logged.?out|sign.?in|HTTP 401|HTTP 403/i.test(message)
        )
          throw new Error(message || 'YouTube authentication or verification is required');
      };
      const readerError = (value) => {
        if (!value || !value.error) return false;
        fatal(value);
        return true;
      };
      const addMatched = (channelId, videoId, entry, video) => {
        if (!matched.has(channelId)) matched.set(channelId, []);
        const rows = matched.get(channelId);
        if (rows.length >= 3 || rows.some((row) => row.video_id === videoId)) return;
        rows.push({
          title: video?.title || entry?.title || videoId,
          url: entry?.url || 'https://www.youtube.com/watch?v=' + videoId,
          video_id: videoId,
          ...(video?.description ? { description: video.description } : {}),
          ...(entry?.views || video?.views ? { views_text: entry?.views || video.views } : {}),
          ...(entry?.published || video?.publishDate
            ? { published_text: entry?.published || video.publishDate }
            : {}),
        });
        const completedChannel = completed.find((channel) => channel.platform_id === channelId);
        if (completedChannel) completedChannel.matched_videos = rows.slice(0, 3);
      };
      const ingestEntries = async (entries) => {
        const pending = [];
        for (const pair of entries || []) {
          const [videoId, entry] = pair || [];
          if (!videoId || seenVideos.has(videoId) || seenVideos.size >= maxVideos) continue;
          seenVideos.add(videoId);
          pending.push([String(videoId), entry || {}]);
        }
        for (let i = 0; i < pending.length; i += 3) {
          if (Date.now() >= deadline) break;
          const batch = pending.slice(i, i + 3);
          const details = await Promise.all(
            batch.map(async ([videoId, entry]) => ({
              videoId,
              entry,
              video: await readers.getVideo(videoId, timeoutMs()),
            })),
          );
          for (const item of details) {
            if (readerError(item.video)) {
              warningList.push(
                'Could not read video ' +
                  item.videoId +
                  ': ' +
                  String(item.video.message || item.video.error),
              );
              continue;
            }
            const video = item.video || {};
            if (!video.channelId) {
              warningList.push('Video ' + item.videoId + ' did not expose a channelId');
              continue;
            }
            const channelId = String(video.channelId);
            addMatched(channelId, item.videoId, item.entry, video);
            if (!channels.has(channelId))
              channels.set(channelId, { platform_id: channelId, name: video.channel || channelId });
          }
          await profileChannels();
          if (completed.length >= limit) break;
        }
      };
      const fetchChannel = async (channelId) => {
        const channel = await readers.getChannel(channelId, 3, timeoutMs());
        if (readerError(channel)) {
          warningList.push(
            'Could not read channel ' + channelId + ': ' + String(channel.message || channel.error),
          );
          return null;
        }
        const recent = (channel.recentVideos || []).slice(0, 3).map((video) => {
          const videoId = readers.parseVideoId(video.url || '');
          return {
            title: video.title || videoId,
            url: video.url || 'https://www.youtube.com/watch?v=' + videoId,
            video_id: videoId,
            ...(video.views ? { views_text: video.views } : {}),
          };
        });
        const result = {
          platform_id: channelId,
          name: channel.name || channels.get(channelId)?.name || channelId,
          url: channel.url || 'https://www.youtube.com/channel/' + channelId,
          matched_videos: (matched.get(channelId) || []).slice(0, 3),
          recent_videos: recent,
        };
        if (channel.image_url) result.image_url = channel.image_url;
        if (channel.description) result.description = channel.description;
        if (channel.subscribers) result.subscriber_text = channel.subscribers;
        if (channel.warnings?.length) result.warnings = channel.warnings;
        return result;
      };
      const profileChannels = async () => {
        const pending = Array.from(channels.keys()).filter((id) => !attemptedChannels.has(id));
        let offset = 0;
        while (offset < pending.length && completed.length < limit && Date.now() < deadline) {
          const batchSize = Math.min(3, limit - completed.length);
          const batch = pending.slice(offset, offset + batchSize);
          offset += batch.length;
          batch.forEach((id) => attemptedChannels.add(id));
          completed.push(...(await Promise.all(batch.map(fetchChannel))).filter(Boolean));
        }
      };

      let page = await readers.searchPage({
        continuation: null,
        sapisid,
        timeoutMs: timeoutMs(),
        resultType: 'video',
      });
      if (readerError(page))
        throw new Error(String(page.message || page.error || 'YouTube search failed'));
      let cursor = page.nextCursor;
      let pages = 1;
      await ingestEntries(page.entries);

      while (
        completed.length < limit &&
        cursor &&
        pages < maxPages &&
        seenVideos.size < maxVideos &&
        Date.now() < deadline
      ) {
        if (seenCursors.has(cursor))
          throw new Error(
            localized('repeatedCursor', 'YouTube search returned a repeated continuation cursor'),
          );
        seenCursors.add(cursor);
        page = await readers.searchPage({
          continuation: cursor,
          sapisid,
          timeoutMs: timeoutMs(),
          resultType: 'video',
        });
        if (readerError(page))
          throw new Error(String(page.message || page.error || 'YouTube search failed'));
        pages += 1;
        await ingestEntries(page.entries);
        cursor = page.nextCursor;
      }
      const stopReason =
        completed.length >= limit
          ? 'limit_reached'
          : Date.now() >= deadline
            ? 'time_limit'
            : seenVideos.size >= maxVideos
              ? 'video_limit'
              : pages >= maxPages && cursor
                ? 'page_limit'
                : 'exhausted';
      return {
        channels: completed.slice(0, limit),
        has_more: Boolean(cursor) && stopReason !== 'exhausted',
        stop_reason: stopReason,
        warnings: warningList,
      };
    })().then(
      (result) => ({ ok: true, result }),
      (error) => ({ ok: false, error: String(error?.message || error) }),
    );
  }

  async function discoverKols(query, limit) {
    const text = String(query || '').trim();
    const count = Number(limit || 20);
    const get = (key) => root.chrome.i18n.getMessage(key) || key;
    if (!text) throw new Error(get('youtubeQueryRequired'));
    if (!Number.isInteger(count) || count < 1 || count > 50)
      throw new Error(get('youtubeLimitRange'));
    const tab = await root.chrome.tabs.create({
      url:
        'https://www.youtube.com/results?search_query=' +
        encodeURIComponent(text) +
        '&sp=EgIQAQ%3D%3D',
      active: false,
    });
    try {
      const started = Date.now();
      while (Date.now() - started < 15000) {
        const current = await root.chrome.tabs.get(tab.id);
        if (current.status === 'complete' && root.OneCrewYoutubePlatform.supportsUrl(current.url)) {
          try {
            const envelope = await execute(
              tab.id,
              youtubeDiscoverKols,
              [text, count, await cookies(), messages()],
              READER_FILES,
            );
            if (!envelope || typeof envelope !== 'object')
              throw new Error('YouTube discovery returned no serializable result');
            if (envelope.ok === false)
              throw new Error(String(envelope.error || 'YouTube discovery failed'));
            if (envelope.ok !== true || !Object.prototype.hasOwnProperty.call(envelope, 'result'))
              throw new Error('YouTube discovery returned an invalid result envelope');
            return envelope.result;
          } catch (error) {
            if (!String(error?.message || '').includes(READINESS_ERROR_MARKER)) throw error;
          }
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      throw new Error(get('youtubeSearchReadinessTimeout'));
    } finally {
      try {
        await root.chrome.tabs.remove(tab.id);
      } catch (_) {
        // The owned tab may already have been closed by the user.
      }
    }
  }

  root.OneCrewYoutubePlatform = {
    supportsUrl: (value) => {
      try {
        const parsed = new URL(value);
        return (
          parsed.protocol === 'https:' &&
          (parsed.hostname === 'www.youtube.com' || parsed.hostname === 'youtube.com') &&
          !parsed.username &&
          !parsed.password &&
          !parsed.port
        );
      } catch (_) {
        return false;
      }
    },
    operations: { 'youtube.discover_kols': (input) => discoverKols(input.query, input.limit) },
    cookies,
    execute,
    messages,
    youtubeDiscoverKols,
    discoverKols,
    readinessErrorMarker: READINESS_ERROR_MARKER,
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
