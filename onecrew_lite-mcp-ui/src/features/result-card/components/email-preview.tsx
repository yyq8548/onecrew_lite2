import { ChevronRight } from "lucide-react";

import { Button } from "@mcp-ui/components/ui/button";
import { displayLabel } from "@mcp-ui/lib/display-label";
import { useAppLocale, type CopyKey } from "@mcp-ui/lib/i18n";
import { emailInitial, type EmailDetailKey, type EmailPreviewModel } from "../email-preview";
import { useCardDetails } from "./text-details";

const DETAIL_COPY: Record<Exclude<EmailDetailKey, "other">, CopyKey> = {
  from: "mailFrom", to: "mailTo", date: "mailDate", subject: "mailSubject",
};

/** The addressing internals of a prepared message, read on their own. */
export function EmailDetails({ model }: { model: EmailPreviewModel }) {
  const { t, locale } = useAppLocale();
  return <dl className="mail-details">
    {model.details.map((detail, index) => <div key={`${detail.key}:${index}`} className="contents">
      <dt>{detail.key === "other" ? displayLabel(detail.label ?? "", locale) : t(DETAIL_COPY[detail.key])}</dt>
      <dd>{detail.value}</dd>
    </div>)}
  </dl>;
}

/** A prepared message, read the way a mail client shows one.
 *
 * The body is shown whole. It used to be cut into 160-character pages behind a
 * previous/next pager, which is a list control: a letter became "page 1 of 2"
 * and the reader had to click to finish a sentence. Nothing about a message is
 * a list. The card's own content region scrolls once the Host caps its height,
 * so length is handled where length belongs and the action bar stays in frame.
 *
 * The body keeps the wire value's own line breaks through `white-space:
 * pre-wrap`; it is never injected as markup, so a body containing angle
 * brackets stays text.
 *
 * Details used to expand under the addressing, which added height. This Host
 * keeps its own frame height, so the height a disclosure adds is height the
 * frame does not have: the message and the action bar under it were pushed out
 * of view to make room. The details now replace the card's body and carry the
 * way back, and they are mounted only while shown, so the collapsed card
 * carries none of the operation's internals. */
export function EmailPreviewCard({ model }: { model: EmailPreviewModel }) {
  const { t } = useAppLocale();
  const { openView } = useCardDetails();
  const recipients = model.to.join(t("listSeparator"));

  return <section className="mail-preview" aria-label={t("mailPreview")}>
    <div className="mail-preview-head">
      <span className="mail-avatar" aria-hidden="true">{emailInitial(model.from)}</span>
      <span className="mail-identity">
        <span className="mail-sender">
          <span className="font-medium [overflow-wrap:anywhere]">{model.from?.name ?? model.from?.address ?? t("mailUnknownSender")}</span>
          {model.from?.name && model.from.address && <span className="mail-sender-address">{`<${model.from.address}>`}</span>}
        </span>
        <span className="mail-recipients">
          <span>{t("mailTo")}</span><strong>{recipients}</strong>
          <Button type="button" variant="ghost" size="sm" className="mail-detail-trigger size-6 rounded-full p-0"
            aria-label={t("mailViewDetails")}
            onClick={() => openView(t("mailDetailsTitle"), <EmailDetails model={model} />)}>
            <ChevronRight aria-hidden="true" className="size-4" />
          </Button>
        </span>
      </span>
      {model.date && <span className="mail-time">{model.date}</span>}
    </div>
    <p className="mail-body">{model.body}</p>
  </section>;
}
