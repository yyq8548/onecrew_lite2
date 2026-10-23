import { z } from "zod";

export const instagramVisiblePosts = z.array(z.strictObject({
  code: z.string().regex(/^[A-Za-z\d_-]{1,32}$/u),
  kind: z.enum(["p", "reel"]),
  url: z.string(),
  preview: z.string().refine(value => value.isWellFormed() && [...value].length <= 500 && !/[\p{Cc}&&[^\n\r\t]]/v.test(value)),
})).max(6);

export function instagramVisiblePostsMatchUsername(posts: z.infer<typeof instagramVisiblePosts> | undefined, username: string): boolean {
  return posts === undefined || posts.every(post => post.url === `https://www.instagram.com/${username}/${post.kind}/${post.code}/`);
}
