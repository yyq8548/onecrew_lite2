import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { instagramProfilePlan, tiktokProfilePlan } from "./profiles.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const id = z.string().regex(/^[1-9][0-9]{0,24}$/u);
export function tiktokProfileIdentityPlan(username: string) {
  if (!/^[a-z\d_.]{1,24}$/iu.test(username)) throw new OpenCliError("opencli_invalid_input");
  return { site: "onecrew-tiktok-profile", name: "identity", args: ["-f", "json", "--", username], addons: ["onecrew-tiktok-profile"], maximumBytes: 24000,
    decode(raw: unknown) {
      const parsed = z.array(z.object({ id })).length(1).safeParse(raw);
      if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
      return tiktokProfilePlan(username, parsed.data[0]!.id).decode(raw);
    },
  } satisfies ReviewedOpenCliRead<unknown>;
}
export function instagramProfileIdentityPlan(username: string) {
  if (!/^[a-z\d_.]{1,30}$/u.test(username)) throw new OpenCliError("opencli_invalid_input");
  const profile = instagramProfilePlan(username, `https://www.instagram.com/${username}/`);
  return { site: "onecrew-instagram-profile", name: "identity", args: ["-f", "json", "--", username], addons: ["onecrew-browser-search"], maximumBytes: 16000, foregroundBrowser: true,
    decode(raw: unknown) {
      return profile.decode(raw);
    },
  } satisfies ReviewedOpenCliRead<unknown>;
}
