// YouTube channel extraction adapted from jackwener/OpenCLI.
// Source: clis/youtube/channel.js @ 8271afc67e8504bda94c147f446ee29775d08274 (Apache-2.0).
// See ../../../THIRD_PARTY_NOTICES.md and ../../licenses/APACHE-2.0.txt.
(function (root) {
  'use strict';

  // prettier-ignore
  function extractSelectedRichGridContents(browseData) {
    const tabs = browseData?.contents?.twoColumnBrowseResultsRenderer?.tabs || [];
    const readRichGrid = (tab) => tab?.tabRenderer?.content?.richGridRenderer?.contents;
    const selectedTab = tabs.find(t => t?.tabRenderer?.selected);
    const selectedContents = readRichGrid(selectedTab);
    if (Array.isArray(selectedContents)) return selectedContents;
    const fallbackContents =
      readRichGrid(tabs.find(t => {
        const contents = readRichGrid(t);
        return Array.isArray(contents) && contents.length > 0;
      })) || readRichGrid(tabs.find(t => Array.isArray(readRichGrid(t))));
    return Array.isArray(fallbackContents) ? fallbackContents : [];
  }

  // prettier-ignore
  function parseVideoItem(item) {
    const content = item?.richItemRenderer?.content || item || {};
    const normalizeId = (value) => {
      const id = String(value || '').trim();
      return /^[A-Za-z0-9_-]+$/.test(id) ? id : '';
    };
    // New lockupViewModel format
    const lvm = content.lockupViewModel;
    if (lvm && lvm.contentType === 'LOCKUP_CONTENT_TYPE_VIDEO') {
      const id = normalizeId(lvm.contentId);
      const meta = lvm.metadata?.lockupMetadataViewModel;
      const title = meta?.title?.content || '';
      if (!id || !title) return null;
      const rows = meta?.metadata?.contentMetadataViewModel?.metadataRows || [];
      const parts = (rows[0]?.metadataParts || []).map(p => p.text?.content).filter(Boolean);
      let duration = '';
      for (const ov of (lvm.contentImage?.thumbnailViewModel?.overlays || [])) {
        for (const b of (ov.thumbnailBottomOverlayViewModel?.badges || [])) {
          if (b.thumbnailBadgeViewModel?.text) duration = b.thumbnailBadgeViewModel.text;
        }
      }
      return {
        title,
        duration,
        views: parts.join(' | '),
        url: 'https://www.youtube.com/watch?v=' + id,
      };
    }
    // Legacy videoRenderer format
    const v = content.videoRenderer || content.gridVideoRenderer;
    if (v) {
      const id = normalizeId(v.videoId);
      const title = v.title?.runs?.[0]?.text || v.title?.simpleText || '';
      if (!id || !title) return null;
      return {
        title,
        duration:
          v.lengthText?.simpleText ||
          v.thumbnailOverlays?.find(o => o.thumbnailOverlayTimeStatusRenderer)
            ?.thumbnailOverlayTimeStatusRenderer?.text?.simpleText ||
          '',
        views:
          (v.shortViewCountText?.simpleText || '') +
          (v.publishedTimeText?.simpleText ? ' | ' + v.publishedTimeText.simpleText : ''),
        url: 'https://www.youtube.com/watch?v=' + id,
      };
    }
    return null;
  }

  async function requestJson(path, body, timeoutMs) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      let response;
      try {
        response = await root.fetch(path, {
          method: 'POST',
          credentials: 'include',
          signal: controller.signal,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
      } catch (error) {
        if (error?.name === 'AbortError' || error?.name === 'TimeoutError') {
          return { error: 'timeout', message: 'YouTube channel request timed out' };
        }
        return { error: 'transport', message: String(error?.message || error) };
      }
      if (!response.ok)
        return { error: 'http', message: 'Channel API returned HTTP ' + response.status };
      let data;
      try {
        data = await response.json();
      } catch (error) {
        return { error: 'json', message: String(error?.message || error) };
      }
      if (data?.error) {
        return {
          error: 'api',
          message: data.error.message || 'YouTube channel API returned an error',
        };
      }
      return { data };
    } finally {
      clearTimeout(timer);
    }
  }

  async function getChannel(channelId, limit = 3, timeoutMs = 15000) {
    const id = String(channelId || '');
    const maxVideos = Math.min(Number(limit) || 3, 30);
    const cfg = root.ytcfg?.data_ || {};
    const apiKey = cfg.INNERTUBE_API_KEY;
    const context = cfg.INNERTUBE_CONTEXT;
    if (!apiKey || !context) {
      return { error: 'config', message: 'YouTube channel bootstrap data not found' };
    }
    const apiPath = '/youtubei/v1/browse?key=' + encodeURIComponent(apiKey) + '&prettyPrint=false';
    let browseId = id;
    if (id.startsWith('@')) {
      const resolved = await requestJson(
        '/youtubei/v1/navigation/resolve_url?key=' +
          encodeURIComponent(apiKey) +
          '&prettyPrint=false',
        { context, url: 'https://www.youtube.com/' + id },
        timeoutMs,
      );
      if (resolved.error) return resolved;
      browseId = resolved.data?.endpoint?.browseEndpoint?.browseId || id;
    }

    const channelResponse = await requestJson(apiPath, { context, browseId }, timeoutMs);
    if (channelResponse.error) return channelResponse;
    const data = channelResponse.data;

    const errorAlert = (data?.alerts || []).find((alert) => alert?.alertRenderer?.type === 'ERROR');
    if (errorAlert) {
      const text = errorAlert.alertRenderer?.text;
      const message =
        text?.simpleText ||
        (Array.isArray(text?.runs) ? text.runs.map((run) => run?.text || '').join('') : '') ||
        'YouTube channel is unavailable';
      return { error: 'channel_unavailable', message };
    }

    // Channel metadata
    const metadata = data?.metadata?.channelMetadataRenderer;
    if (
      !metadata ||
      typeof metadata.title !== 'string' ||
      !metadata.title.trim() ||
      typeof metadata.externalId !== 'string' ||
      !metadata.externalId.trim()
    ) {
      return {
        error: 'channel_unavailable',
        message: 'YouTube channel metadata is unavailable',
      };
    }
    const header = data.header?.pageHeaderRenderer || data.header?.c4TabbedHeaderRenderer || {};

    // Subscriber count from header
    let subscriberCount = '';
    try {
      const rows =
        header.content?.pageHeaderViewModel?.metadata?.contentMetadataViewModel?.metadataRows || [];
      for (const row of rows) {
        for (const part of row.metadataParts || []) {
          const text = part.text?.content || '';
          if (text.includes('subscriber')) subscriberCount = text;
        }
      }
    } catch {
      // Some channel header variants omit structured subscriber metadata.
    }
    // Fallback for old c4TabbedHeaderRenderer format
    if (!subscriberCount && header.subscriberCountText?.simpleText) {
      subscriberCount = header.subscriberCountText.simpleText;
    }

    const tabs = data.contents?.twoColumnBrowseResultsRenderer?.tabs || [];
    const videosTab = tabs.find((t) => {
      const tab = t.tabRenderer;
      const url = tab?.endpoint?.commandMetadata?.webCommandMetadata?.url || '';
      return (
        tab?.tabIdentifier === 'VIDEOS' ||
        url.split('?')[0].endsWith('/videos') ||
        tab?.title === 'Videos'
      );
    });
    const videosTabParams = videosTab?.tabRenderer?.endpoint?.browseEndpoint?.params;
    const recentVideos = [];
    let recentVideosUnavailable = true;
    if (videosTabParams) {
      const videosResponse = await requestJson(
        apiPath,
        { context, browseId, params: videosTabParams },
        timeoutMs,
      );
      if (videosResponse.error) return videosResponse;
      const responseTabs =
        videosResponse.data?.contents?.twoColumnBrowseResultsRenderer?.tabs || [];
      const isVideosTab = (tab) => {
        const tabRenderer = tab?.tabRenderer;
        const url = tabRenderer?.endpoint?.commandMetadata?.webCommandMetadata?.url || '';
        const path = url.split('?')[0];
        const params = tabRenderer?.endpoint?.browseEndpoint?.params;
        return (
          tabRenderer?.tabIdentifier === 'VIDEOS' ||
          path.endsWith('/videos') ||
          params === videosTabParams ||
          tabRenderer?.title === 'Videos'
        );
      };
      const selectedVideosTab = responseTabs.find(
        (tab) => tab?.tabRenderer?.selected && isVideosTab(tab),
      );
      const selectedContents = selectedVideosTab?.tabRenderer?.content?.richGridRenderer?.contents;
      if (Array.isArray(selectedContents)) {
        recentVideosUnavailable = false;
        const richGrid = extractSelectedRichGridContents(videosResponse.data);
        for (const item of richGrid) {
          if (recentVideos.length >= maxVideos) break;
          const parsed = parseVideoItem(item);
          if (parsed) recentVideos.push(parsed);
        }
      }
    }

    const avatar =
      metadata.avatar?.thumbnails?.slice(-1)?.[0]?.url ||
      header.avatar?.thumbnails?.slice(-1)?.[0]?.url ||
      '';
    const result = {
      name: metadata.title || '',
      channelId: metadata.externalId || browseId,
      handle: metadata.vanityChannelUrl?.split('/').pop() || '',
      description: (metadata.description || '').substring(0, 500),
      subscribers: subscriberCount,
      url: metadata.channelUrl || 'https://www.youtube.com/channel/' + browseId,
      keywords: metadata.keywords || '',
      recentVideos,
    };
    if (avatar) result.image_url = avatar;
    if (recentVideosUnavailable) result.warnings = ['recent_videos_unavailable'];
    return result;
  }

  root.OneCrewYoutubeReaders = root.OneCrewYoutubeReaders || {};
  Object.assign(root.OneCrewYoutubeReaders, {
    extractSelectedRichGridContents,
    getChannel,
    parseVideoItem,
  });
})(globalThis);
