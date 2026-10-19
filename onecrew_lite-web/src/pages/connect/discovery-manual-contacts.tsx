import { useTranslation } from "react-i18next";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { DiscoveryCandidate } from "@/lib/opencli-discovery";
import { authorProfileLink } from "@/lib/discovery-author-groups";
import { redditManualContactLinks } from "@/lib/discovery-manual-contact-links";

export function DiscoveryManualContacts({ candidates }: { candidates: DiscoveryCandidate[] }) {
  const { t } = useTranslation("shell");
  const rows = candidates.flatMap(candidate => {
    const profile = authorProfileLink(candidate.profile_url);
    const hidden = candidate.evidence.contact_status?.manual_email_required && profile
      ? [{ candidate, type: "hidden" as const, url: profile, source: profile,
          status: "opencliPilot.manualContactsPending" as const }] : [];
    const linkedin = candidate.platform === "linkedin" && candidate.evidence.manual_profile_contact === true && profile
      ? [{ candidate, type: "linkedin" as const, url: profile, source: profile,
          status: "opencliPilot.manualContactsLinkedInPending" as const }] : [];
    const redditLinks = redditManualContactLinks(candidate);
    const reddit = redditLinks
      ? [
          { candidate, type: "redditAuthor" as const, url: redditLinks.profile, source: redditLinks.post,
            status: "opencliPilot.manualContactsLinkedInPending" as const },
          { candidate, type: "redditModerators" as const, url: redditLinks.post, source: redditLinks.post,
            status: "opencliPilot.manualContactsRedditModeratorsPending" as const },
        ] : [];
    const wechat = (candidate.evidence.manual_wechat_accounts ?? []).flatMap(account => {
      const source = authorProfileLink(account.source_url);
      if (!source || account.native_author_id !== candidate.platform_account_id) return [];
      return [{ candidate, type: account.kind, value: account.value, url: source, source, status: "opencliPilot.socialAccountsPublished" as const }];
    });
    const seen = new Set<string>();
    const accounts = (candidate.evidence.associated_social_accounts ?? []).flatMap(account => {
      const url = authorProfileLink(account.normalized_value);
      const source = authorProfileLink(account.source_url);
      const key = `${url}\n${source}`;
      if (!url || !source || account.native_author_id !== candidate.platform_account_id || seen.has(key)) return [];
      seen.add(key);
      return [{ candidate, type: "social" as const, url, source,
        status: account.attribution === "author_published"
          ? "opencliPilot.socialAccountsPublished" as const : "opencliPilot.socialAccountsUnconfirmed" as const }];
    });
    return [...hidden, ...linkedin, ...reddit, ...accounts, ...wechat];
  });
  if (rows.length === 0) return null;
  return <section aria-label={t("opencliPilot.manualContactsTitle")} className="space-y-3">
    <h3 className="font-semibold">{t("opencliPilot.manualContactsTitle")}</h3>
    <p className="text-sm text-muted-foreground">{t("opencliPilot.manualContactsHelp")}</p>
    <Table><TableHeader><TableRow>
      <TableHead>{t("opencliPilot.tableAuthor")}</TableHead>
      <TableHead>{t("opencliPilot.manualContactsType")}</TableHead>
      <TableHead>{t("opencliPilot.manualContactsAction")}</TableHead>
      <TableHead>{t("opencliPilot.socialAccountsSource")}</TableHead>
      <TableHead>{t("opencliPilot.tableStatus")}</TableHead>
    </TableRow></TableHeader><TableBody>
      {rows.map(row => {
        const { candidate, type, url, source, status } = row;
        const link = authorProfileLink(candidate.profile_url);
        return <TableRow key={`${candidate.id}:${type}:${url}:${source}:${"value" in row ? row.value : ""}`}>
          <TableCell>{link ? <a href={link} target="_blank" rel="noreferrer" className="text-primary underline">{candidate.display_name}</a> : candidate.display_name}</TableCell>
          <TableCell>{t(type === "wechat" ? "opencliPilot.manualContactsWechat" : type === "wechat_official" ? "opencliPilot.manualContactsWechatOfficial" : type === "hidden" ? "opencliPilot.manualContactsHidden" : type === "linkedin" ? "opencliPilot.manualContactsLinkedIn" : type === "redditAuthor" ? "opencliPilot.manualContactsRedditAuthor" : type === "redditModerators" ? "opencliPilot.manualContactsRedditModerators" : "opencliPilot.socialAccountsTitle")}</TableCell>
          <TableCell className="break-all whitespace-normal">{"value" in row && <p>{row.value}</p>}<a href={url} target="_blank" rel="noreferrer" className="text-primary underline">{url}</a>
            {type === "hidden" && <p>{t("opencliPilot.manualContactsSteps")}</p>}
            {type === "linkedin" && <p>{t("opencliPilot.manualContactsLinkedInSteps")}</p>}
            {type === "redditAuthor" && <p>{t("opencliPilot.manualContactsRedditAuthorSteps")}</p>}
            {type === "redditModerators" && <p>{t("opencliPilot.manualContactsRedditModeratorsSteps")}</p>}</TableCell>
          <TableCell className="break-all whitespace-normal"><a href={source} target="_blank" rel="noreferrer" className="text-primary underline">{source}</a></TableCell>
          <TableCell>{t(status)}</TableCell>
        </TableRow>;
      })}
    </TableBody></Table>
  </section>;
}
