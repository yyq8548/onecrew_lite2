// 一条累积邮件链的渲染:每轮一张抬头卡 + 这一轮说的话。
//
// 撰写区和阅读区共用这一份。邮件回复是累积的,越往下 `>` 越深;把那段
// 文本原样倒出来,读者看到的是一堵符号墙。`parseEmailChain` 负责还原成
// 「谁在什么时候说了什么」,这里只负责画。
//
// 单独成文件是为了避开环:撰写面板已经 import 阅读区,反过来再引一次
// 就成环了。

import { useTranslation } from "react-i18next";

import { formatDate, type EmailMessageView } from "@/lib/creator";
import type { ChainTurn } from "@/components/mail/quoted-body";

export function EmailChainTurns({
  turns,
  first,
}: {
  turns: ChainTurn[];
  /**
   * 最新一轮对应的那封信。给了就用它的元数据画第一张抬头卡——正文里
   * 的复述可能被客户端改写过,元数据不会。阅读区已经在上面画过抬头,
   * 传的是 `slice(1)` 且不给 `first`,于是每一轮都用自己解析出的抬头。
   */
  first?: EmailMessageView;
}) {
  return (
    <>
      {turns.map((turn, index) => (
        <div key={index} className="space-y-2">
          {index === 0 && first ? (
            <QuotedMessageCard message={first} />
          ) : turn.header ? (
            <div className="grid gap-0.5 rounded-md bg-muted/60 p-3 text-xs leading-5 text-muted-foreground">
              {Object.entries(turn.header).map(([field, value]) => (
                <div key={field}>
                  {field}: {value}
                </div>
              ))}
            </div>
          ) : turn.attribution ? (
            <div className="rounded-md bg-muted/60 p-3 text-xs leading-5 text-muted-foreground">
              {turn.attribution}
            </div>
          ) : null}
          {turn.body && (
            <pre className="whitespace-pre-wrap break-words font-sans text-sm leading-6">
              {turn.body}
            </pre>
          )}
        </div>
      ))}
    </>
  );
}

/** 最新一轮的抬头卡,取自消息元数据而不是正文里的复述。 */
export function QuotedMessageCard({ message }: { message: EmailMessageView }) {
  const { t } = useTranslation("mail");
  return (
    <div className="grid gap-0.5 rounded-md bg-muted/60 p-3 text-xs leading-5 text-muted-foreground">
      <div>{t("message.headers.from")}: {message.direction === "outbound" ? t("common.me") : message.from}</div>
      <div>{t("message.headers.date")}: {formatDate(message.occurred_at)}</div>
      <div>{t("message.headers.subject")}: {message.subject || t("common.noSubject")}</div>
      <div>{t("message.headers.to")}: {message.to.join(", ") || "—"}</div>
    </div>
  );
}
