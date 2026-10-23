export function observeYoutubeSearchAuthors() {
  const authors = new Map();
  function visit(value) {
    if (!value || typeof value !== 'object') return;
    const video = value.videoRenderer;
    if (video?.videoId) {
      const byline = video.ownerText ?? video.shortBylineText ?? video.longBylineText;
      const ids = (byline?.runs ?? []).map(run => run.navigationEndpoint?.browseEndpoint?.browseId).filter(id => /^UC[A-Za-z\d_-]{22}$/.test(id ?? ''));
      if (ids.length && new Set(ids).size === 1) {
        const previous = authors.get(video.videoId);
        authors.set(video.videoId, authors.has(video.videoId) && previous !== ids[0] ? null : ids[0]);
      }
    }
    for (const child of Object.values(value)) visit(child);
  }
  visit(window.ytInitialData);
  return [...authors].filter(([, author]) => author !== null).map(([video_id, channel_id]) => ({ video_id, channel_id }));
}
