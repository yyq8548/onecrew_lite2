import { localizedPath } from "@/lib/site-locale";

import { useLandingLocale } from "../language-switcher";
import { absoluteUrl } from "./site";

type DocumentMetaProps = {
  canonicalPath: string;
  description: string;
  noIndex?: boolean;
  schema?: unknown;
  title: string;
};

export function DocumentMeta({
  canonicalPath,
  description,
  noIndex = false,
  schema,
  title,
}: DocumentMetaProps) {
  const locale = useLandingLocale();
  const english = locale === "en";
  const canonicalUrl = absoluteUrl(localizedPath(canonicalPath, locale));
  const chineseUrl = absoluteUrl(localizedPath(canonicalPath, "zh"));
  const englishUrl = absoluteUrl(localizedPath(canonicalPath, "en"));
  const socialImage = absoluteUrl("/onecrew-ai-mark.png");
  const socialImageAlt = english ? "OneCrew brand mark" : "OneCrew 品牌标志";

  return (
    <>
      <title>{title}</title>
      <meta name="description" content={description} />
      <meta name="robots" content={noIndex ? "noindex, nofollow" : "index, follow"} />
      <link rel="canonical" href={canonicalUrl} />
      <link rel="alternate" hrefLang="zh-CN" href={chineseUrl} />
      <link rel="alternate" hrefLang="en" href={englishUrl} />
      <link rel="alternate" hrefLang="x-default" href={englishUrl} />
      <meta property="og:title" content={title} />
      <meta property="og:description" content={description} />
      <meta property="og:url" content={canonicalUrl} />
      <meta property="og:type" content="website" />
      <meta property="og:site_name" content="OneCrew" />
      <meta property="og:locale" content={english ? "en_US" : "zh_CN"} />
      <meta property="og:locale:alternate" content={english ? "zh_CN" : "en_US"} />
      <meta property="og:image" content={socialImage} />
      <meta property="og:image:secure_url" content={socialImage} />
      <meta property="og:image:type" content="image/png" />
      <meta property="og:image:width" content="512" />
      <meta property="og:image:height" content="512" />
      <meta property="og:image:alt" content={socialImageAlt} />
      <meta name="twitter:card" content="summary" />
      <meta name="twitter:title" content={title} />
      <meta name="twitter:description" content={description} />
      <meta name="twitter:image" content={socialImage} />
      <meta name="twitter:image:alt" content={socialImageAlt} />
      {schema === undefined ? null : (
        <script
          type="application/ld+json"
          dangerouslySetInnerHTML={{
            __html: JSON.stringify(schema).replaceAll("<", "\\u003c"),
          }}
        />
      )}
    </>
  );
}
