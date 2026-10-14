import { useAccessLoginUrl } from "../access-site";
import styles from "./marketing-pages.module.css";
import { useMarketingContent } from "./use-marketing-content";

export function PageCta() {
  const ACCESS_LOGIN_URL = useAccessLoginUrl();
  const { content } = useMarketingContent();

  return (
    <a className={styles.cta} href={ACCESS_LOGIN_URL}>
      {content.common.tryOneCrew}
      <span aria-hidden="true">→</span>
    </a>
  );
}
