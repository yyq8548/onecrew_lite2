import { OpenCliError } from "./errors.ts";

export function creatorProfileSource(site: string, authorId: string, source: string): string {
  const invalid = (): never => { throw new OpenCliError("opencli_invalid_input"); };
  switch (site) {
    case "linkedin": if (source !== `https://www.linkedin.com/in/${authorId}/`) return invalid(); return authorId;
    case "pinterest": { const match = /^https:\/\/www\.pinterest\.com\/([A-Za-z\d_]{1,30})\/$/u.exec(source); return match ? match[1]! : invalid(); }
    case "bluesky": if (source !== `https://bsky.app/profile/${authorId}`) return invalid(); return "";
    case "weibo": if (source !== `https://weibo.com/u/${authorId}`) return invalid(); return "";
    case "facebook": if (source !== `https://www.facebook.com/profile.php?id=${authorId}`) return invalid(); return "";
    case "bilibili": if (source !== `https://space.bilibili.com/${authorId}`) return invalid(); return "";
    case "instagram": { const match = /^https:\/\/www\.instagram\.com\/([a-z\d_.]{1,30})\/$/u.exec(source); return match ? match[1]! : invalid(); }
    case "tiktok": { const match = /^https:\/\/www\.tiktok\.com\/@([A-Za-z\d_.]{1,24})$/u.exec(source); return match ? match[1]! : invalid(); }
    case "twitter": if (source !== `https://x.com/${authorId}` || authorId !== authorId.toLowerCase()) return invalid(); return authorId;
    case "reddit": { const match = /^https:\/\/www\.reddit\.com\/user\/([A-Za-z\d_-]{3,20})\/$/u.exec(source); return match ? match[1]! : invalid(); }
    default: throw new OpenCliError("opencli_read_command_unavailable");
  }
}
