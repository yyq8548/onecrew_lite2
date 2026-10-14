// Native original text and direct-reply provenance; commenter IDs are unnecessary.
export function projectLanguageSample(data, handle, postId, accountId, contentProjector, identityProjector) {
  const post = contentProjector(data, handle, postId, accountId, identityProjector);
  const comments = [], seen = new Set();
  let inspected = 0;
  const inspect = result => {
    const tweet = result?.__typename === 'TweetWithVisibilityResults' ? result.tweet : result;
    const user = tweet?.core?.user_results?.result;
    const id = tweet?.rest_id;
    const screenName = user?.core?.screen_name ?? user?.legacy?.screen_name;
    const text = tweet?.note_tweet?.note_tweet_results?.result?.text ?? tweet?.legacy?.full_text;
    if (tweet?.__typename !== 'Tweet' || tweet?.legacy?.in_reply_to_status_id_str !== postId
        || tweet.legacy.retweeted_status_result || tweet.retweeted_status_result || seen.has(id) || inspected >= 5) return;
    inspected += 1;
    seen.add(id);
    if (!/^[1-9][0-9]{0,19}$/.test(id ?? '') || !/^[A-Za-z0-9_]{1,15}$/.test(screenName ?? '')
        || typeof text !== 'string' || !text.trim() || text.includes('\0')) return;
    let excerpt = '', bytes = 0;
    for (const character of text) {
      const length = new TextEncoder().encode(character).length;
      if (bytes + length > 2000) break;
      excerpt += character; bytes += length;
    }
    comments.push({source_url: `https://x.com/${screenName.toLowerCase()}/status/${id}`, text: excerpt});
  };
  for (const instruction of data.data.threaded_conversation_with_injections_v2.instructions) {
    for (const entry of instruction.entries ?? []) {
      inspect(entry.content?.itemContent?.tweet_results?.result);
      for (const item of entry.content?.items ?? []) inspect(item.item?.itemContent?.tweet_results?.result);
    }
  }
  return {author_id: accountId, source_url: post.identity.source_url, text: post.text, comments};
}
