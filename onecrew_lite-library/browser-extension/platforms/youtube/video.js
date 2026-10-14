// YouTube video extraction adapted from jackwener/OpenCLI.
// Source: clis/youtube/video.js @ 8271afc67e8504bda94c147f446ee29775d08274 (Apache-2.0).
// See ../../../THIRD_PARTY_NOTICES.md and ../../licenses/APACHE-2.0.txt.
(function (root) {
  'use strict';

  async function getVideo(videoId, timeoutMs = 15000) {
    const id = root.OneCrewYoutubeReaders.parseVideoId(String(videoId || ''));
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      let watchResp;
      try {
        watchResp = await root.fetch('/watch?v=' + encodeURIComponent(id), {
          credentials: 'include',
          signal: controller.signal,
        });
      } catch (error) {
        if (error?.name === 'AbortError' || error?.name === 'TimeoutError') {
          return { error: 'timeout', message: 'YouTube video request timed out' };
        }
        return { error: 'transport', message: String(error?.message || error) };
      }
      if (!watchResp.ok) {
        return { error: 'http', message: 'Watch HTML returned HTTP ' + watchResp.status };
      }

      let html;
      try {
        html = await watchResp.text();
      } catch (error) {
        return { error: 'read', message: String(error?.message || error) };
      }
      const extractJsonAssignmentFromHtml =
        root.OneCrewYoutubeReaders.extractJsonAssignmentFromHtml;
      const player = extractJsonAssignmentFromHtml(html, 'ytInitialPlayerResponse');
      const yt = extractJsonAssignmentFromHtml(html, 'ytInitialData');
      if (!player)
        return { error: 'parse', message: 'ytInitialPlayerResponse not found in watch HTML' };

      const details = player.videoDetails || {};
      const microformat = player.microformat?.playerMicroformatRenderer || {};
      const contents = yt?.contents?.twoColumnWatchNextResults?.results?.results?.contents || [];

      // Try to get full description from watch bootstrap data
      let fullDescription = details.shortDescription || '';
      try {
        if (contents) {
          for (const c of contents) {
            const desc = c.videoSecondaryInfoRenderer?.attributedDescription?.content;
            if (desc) {
              fullDescription = desc;
              break;
            }
          }
        }
      } catch {
        // Optional description content is absent on some watch pages.
      }

      // Get like count if available
      let likes = '';
      try {
        if (contents) {
          for (const c of contents) {
            const buttons = c.videoPrimaryInfoRenderer?.videoActions?.menuRenderer?.topLevelButtons;
            if (buttons) {
              for (const b of buttons) {
                const toggle =
                  b.segmentedLikeDislikeButtonViewModel?.likeButtonViewModel?.likeButtonViewModel
                    ?.toggleButtonViewModel?.toggleButtonViewModel?.defaultButtonViewModel
                    ?.buttonViewModel;
                if (toggle?.title) {
                  likes = toggle.title;
                  break;
                }
              }
            }
          }
        }
      } catch {
        // Optional like metadata is absent on some watch pages.
      }

      // Get publish date
      const publishDate =
        microformat.publishDate || microformat.uploadDate || details.publishDate || '';

      // Get category
      const category = microformat.category || '';

      // Get channel subscriber count if available
      let subscribers = '';
      try {
        if (contents) {
          for (const c of contents) {
            const owner =
              c.videoSecondaryInfoRenderer?.owner?.videoOwnerRenderer?.subscriberCountText
                ?.simpleText;
            if (owner) {
              subscribers = owner;
              break;
            }
          }
        }
      } catch {
        // Optional subscriber metadata is absent on some watch pages.
      }

      // 播放门禁信号：会员专享（channel membership）/ 付费点播等视频 metadata 照常
      // 可见，但视频流拿不到——playabilityStatus.status != 'OK'。reason 文本是本地化
      // 的（中文 cookie 下是中文），所以 membersOnly 用 watch HTML 里 locale 无关的
      // BADGE_STYLE_TYPE_MEMBERS_ONLY 徽标枚举判定，下游不要去 parse reason。
      const ps = player.playabilityStatus || {};
      const membersOnly = html.indexOf('BADGE_STYLE_TYPE_MEMBERS_ONLY') !== -1;

      return {
        title: details.title || '',
        channel: details.author || '',
        channelId: details.channelId || '',
        videoId: details.videoId || '',
        views: details.viewCount || '',
        likes,
        subscribers,
        duration: details.lengthSeconds ? details.lengthSeconds + 's' : '',
        publishDate,
        category,
        description: fullDescription,
        keywords: (details.keywords || []).join(', '),
        isLive: details.isLiveContent || false,
        thumbnail: details.thumbnail?.thumbnails?.slice(-1)?.[0]?.url || '',
        playabilityStatus: ps.status || '',
        playabilityReason: ps.reason || '',
        membersOnly,
      };
    } catch (error) {
      return { error: 'parse', message: String(error?.message || error) };
    } finally {
      clearTimeout(timer);
    }
  }

  root.OneCrewYoutubeReaders = root.OneCrewYoutubeReaders || {};
  Object.assign(root.OneCrewYoutubeReaders, { getVideo });
})(globalThis);
