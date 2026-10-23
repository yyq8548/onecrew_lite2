/** Official entry pages; related search commands share the same platform. */
const websites: Record<string, string> = {
  youtube: "https://www.youtube.com/", tiktok: "https://www.tiktok.com/",
  instagram: "https://www.instagram.com/", twitter: "https://x.com/",
  linkedin: "https://www.linkedin.com/", facebook: "https://www.facebook.com/",
  bilibili: "https://www.bilibili.com/", weixin: "https://weixin.sogou.com/",
  douyin: "https://www.douyin.com/",
  weibo: "https://weibo.com/", medium: "https://medium.com/",
  substack: "https://substack.com/", "apple-podcasts": "https://podcasts.apple.com/",
  tieba: "https://tieba.baidu.com/", bluesky: "https://bsky.app/",
  jike: "https://web.okjike.com/", hackernews: "https://news.ycombinator.com/",
  "linux-do": "https://linux.do/", nowcoder: "https://www.nowcoder.com/",
  hupu: "https://www.hupu.com/",
  stackoverflow: "https://stackoverflow.com/", "linkedin-learning": "https://www.linkedin.com/learning/",
  archive: "https://archive.org/", weread: "https://weread.qq.com/",
  google: "https://www.google.com/", duckduckgo: "https://duckduckgo.com/", yahoo: "https://www.yahoo.com/",
  dribbble: "https://dribbble.com/", douban: "https://www.douban.com/",
  v2ex: "https://www.v2ex.com/", devto: "https://dev.to/",
  lesswrong: "https://www.lesswrong.com/", lobsters: "https://lobste.rs/",
  juejin: "https://juejin.cn/", producthunt: "https://www.producthunt.com/",
  hf: "https://huggingface.co/",
};

export function platformWebsite(platform: string): string | undefined {
  return websites[platform] ?? websites[platform.replace(/-(salesnav|shots|designers|images|users|books|music|node|tag|hot|posts|spaces|models|datasets)$/, "")];
}
