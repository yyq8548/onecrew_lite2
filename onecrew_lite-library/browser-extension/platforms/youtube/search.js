// YouTube search extraction adapted from jackwener/OpenCLI.
// Source: clis/youtube/search.js @ 8271afc67e8504bda94c147f446ee29775d08274 (Apache-2.0).
// See ../../../THIRD_PARTY_NOTICES.md and ../../licenses/APACHE-2.0.txt.
(function (root) {
  'use strict';

  function readText(value) {
    if (!value) return '';
    if (typeof value.content === 'string') return value.content;
    if (typeof value.simpleText === 'string') return value.simpleText;
    if (Array.isArray(value.runs)) return value.runs.map((run) => run?.text || '').join('');
    return '';
  }

  function absoluteUrl(path) {
    if (!path) return '';
    return path.startsWith('http') ? path : 'https://www.youtube.com' + path;
  }

  function fromVideo(video) {
    const path =
      video?.navigationEndpoint?.commandMetadata?.webCommandMetadata?.url ||
      (video?.videoId ? '/watch?v=' + video.videoId : '');
    return [
      video?.videoId || '',
      {
        title: readText(video?.title),
        channel: readText(video?.ownerText) || readText(video?.shortBylineText),
        views: readText(video?.viewCountText) || readText(video?.shortViewCountText),
        duration: readText(video?.lengthText) || 'LIVE',
        published: readText(video?.publishedTimeText),
        url: absoluteUrl(path),
      },
    ];
  }

  function fromLegacyShort(reel) {
    return [
      reel?.videoId || '',
      {
        title: readText(reel?.headline),
        channel: readText(
          reel?.navigationEndpoint?.reelWatchEndpoint?.overlay?.reelPlayerOverlayRenderer
            ?.reelPlayerHeaderSupportedRenderers?.reelPlayerHeaderRenderer?.channelTitleText,
        ),
        views: readText(reel?.viewCountText),
        duration: 'SHORT',
        published: readText(reel?.publishedTimeText),
        url: reel?.videoId ? 'https://www.youtube.com/shorts/' + reel.videoId : '',
      },
    ];
  }

  function fromShortLockup(short) {
    const command = short?.onTap?.innertubeCommand;
    const videoId =
      command?.reelWatchEndpoint?.videoId ||
      short?.inlinePlayerData?.onVisible?.innertubeCommand?.watchEndpoint?.videoId ||
      '';
    const path =
      command?.commandMetadata?.webCommandMetadata?.url || (videoId ? '/shorts/' + videoId : '');
    return [
      videoId,
      {
        title: readText(short?.overlayMetadata?.primaryText),
        channel: '',
        views: readText(short?.overlayMetadata?.secondaryText),
        duration: 'SHORT',
        published: '',
        url: absoluteUrl(path),
      },
    ];
  }

  function fromChannel(channel) {
    const path =
      channel?.navigationEndpoint?.commandMetadata?.webCommandMetadata?.url ||
      channel?.navigationEndpoint?.browseEndpoint?.canonicalBaseUrl ||
      (channel?.channelId ? '/channel/' + channel.channelId : '');
    const handleCandidate = readText(channel?.subscriberCountText);
    return [
      channel?.channelId || channel?.navigationEndpoint?.browseEndpoint?.browseId || '',
      {
        title: readText(channel?.title),
        channel: handleCandidate.startsWith('@') ? handleCandidate : '',
        views:
          readText(channel?.videoCountText) ||
          (!handleCandidate.startsWith('@') ? handleCandidate : ''),
        duration: 'CHANNEL',
        published: '',
        url: absoluteUrl(path),
      },
    ];
  }

  function fromPlaylist(playlist) {
    const playlistId =
      playlist?.playlistId || playlist?.navigationEndpoint?.watchEndpoint?.playlistId || '';
    return [
      playlistId,
      {
        title: readText(playlist?.title),
        channel: readText(playlist?.shortBylineText) || readText(playlist?.longBylineText),
        views: readText(playlist?.videoCountText),
        duration: 'PLAYLIST',
        published: '',
        url: playlistId ? 'https://www.youtube.com/playlist?list=' + playlistId : '',
      },
    ];
  }

  function fromPlaylistLockup(lockup) {
    const metadata = lockup?.metadata?.lockupMetadataViewModel;
    const parts = (metadata?.metadata?.contentMetadataViewModel?.metadataRows || []).flatMap(
      (row) => (row.metadataParts || []).map((part) => readText(part?.text)).filter(Boolean),
    );
    const playlistId = lockup?.contentId || '';
    return [
      playlistId,
      {
        title: readText(metadata?.title),
        channel: parts[0] || '',
        views: '',
        duration: 'PLAYLIST',
        published: '',
        url: playlistId ? 'https://www.youtube.com/playlist?list=' + playlistId : '',
      },
    ];
  }

  function extractPage(payload, resultType = 'video') {
    const entries = [];
    const pageIds = new Set();
    const cursors = [];
    const messages = [];
    let malformed = 0;

    function push(entry) {
      const [searchId, row] = entry || [];
      if (!searchId) return;
      if (!row?.title || !row?.url) {
        malformed += 1;
        return;
      }
      if (pageIds.has(searchId)) return;
      pageIds.add(searchId);
      entries.push([searchId, row]);
    }

    function visit(value) {
      if (!value || typeof value !== 'object') return;
      if (!resultType || resultType === 'video') {
        if (value.videoRenderer?.videoId) push(fromVideo(value.videoRenderer));
      } else if (resultType === 'shorts') {
        if (value.reelItemRenderer?.videoId) push(fromLegacyShort(value.reelItemRenderer));
        if (value.shortsLockupViewModel) push(fromShortLockup(value.shortsLockupViewModel));
      } else if (resultType === 'channel') {
        if (value.channelRenderer) push(fromChannel(value.channelRenderer));
      } else if (resultType === 'playlist') {
        if (value.playlistRenderer) push(fromPlaylist(value.playlistRenderer));
        if (value.lockupViewModel?.contentType === 'LOCKUP_CONTENT_TYPE_PLAYLIST') {
          push(fromPlaylistLockup(value.lockupViewModel));
        }
      }

      const cursor =
        value.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token;
      if (cursor) cursors.push(cursor);
      const message = readText(value.messageRenderer?.text);
      if (message) messages.push(message);
      for (const child of Object.values(value)) visit(child);
    }

    visit(payload);
    return {
      entries,
      nextCursor: cursors[cursors.length - 1] || null,
      message: messages[0] || '',
      malformed,
    };
  }

  async function searchPage({
    continuation = null,
    sapisid = null,
    timeoutMs = 15000,
    resultType = 'video',
  } = {}) {
    const cfg = root.ytcfg?.data_ || {};
    const apiKey = cfg.INNERTUBE_API_KEY;
    const context = cfg.INNERTUBE_CONTEXT;
    const initialData = root.ytInitialData;
    if (!apiKey || !context || !initialData) {
      return { error: 'config', message: 'YouTube search bootstrap data not found' };
    }
    if (continuation == null) return extractPage(initialData, resultType);

    const signedIn = cfg.LOGGED_IN === true || root.ytcfg?.get?.('LOGGED_IN') === true;
    let authHash = null;
    try {
      if (sapisid) {
        authHash = await root.OneCrewYoutubeReaders.getSapisidHash(
          sapisid,
          'https://www.youtube.com',
        );
      }
      if (signedIn && !authHash) {
        return { error: 'auth', message: 'Signed-in YouTube search session has no SAPISID cookie' };
      }
      const headers = { 'Content-Type': 'application/json' };
      if (authHash) {
        headers.Authorization = authHash;
        headers['X-Origin'] = 'https://www.youtube.com';
      }
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), timeoutMs);
      try {
        const response = await root.fetch(
          '/youtubei/v1/search?key=' + encodeURIComponent(apiKey) + '&prettyPrint=false',
          {
            method: 'POST',
            credentials: 'include',
            signal: controller.signal,
            headers,
            body: JSON.stringify({ context, continuation }),
          },
        );
        if (response.status === 401 || (response.status === 403 && signedIn)) {
          return { error: 'auth', message: 'YouTube search HTTP ' + response.status };
        }
        if (!response.ok)
          return { error: 'http', message: 'YouTube search HTTP ' + response.status };
        let payload;
        try {
          payload = await response.json();
        } catch (error) {
          if (controller.signal.aborted)
            return { error: 'timeout', message: 'YouTube search request timed out' };
          return { error: 'json', message: String(error?.message || error) };
        }
        const upstreamStatus = payload?.error?.status || '';
        const upstreamCode = Number(payload?.error?.code || 0);
        if (upstreamCode === 401 || upstreamStatus === 'UNAUTHENTICATED') {
          return {
            error: 'auth',
            message: upstreamStatus || 'YouTube search authentication failed',
          };
        }
        if (payload?.error) {
          return {
            error: 'api',
            message: upstreamStatus || payload.error.message || 'YouTube search API error',
          };
        }
        if (signedIn && payload?.responseContext?.mainAppWebResponseContext?.loggedOut === true) {
          return { error: 'auth', message: 'YouTube search continuation response is logged out' };
        }
        return extractPage(payload, resultType);
      } catch (error) {
        if (error?.name === 'AbortError' || error?.name === 'TimeoutError') {
          return { error: 'timeout', message: 'YouTube search request timed out' };
        }
        return { error: 'transport', message: String(error?.message || error) };
      } finally {
        clearTimeout(timer);
      }
    } catch (error) {
      return { error: 'transport', message: String(error?.message || error) };
    }
  }

  root.OneCrewYoutubeReaders = root.OneCrewYoutubeReaders || {};
  Object.assign(root.OneCrewYoutubeReaders, {
    extractSearchPage: extractPage,
    searchPage,
  });
})(globalThis);
