import { useMemo, useState } from "react";
import { Send } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { BULK_KEY, listBulk, listBulkRecipients, prepareBulk, controlBulk, readbackBulk, uniqueRecipients } from "@/lib/gmail-bulk";
import type { MailboxDetail } from "@/lib/creator";
import { useCanvasPortal } from "@/lib/canvas-portal";
import { MAIL_ACCESS_CONNECTIONS_KEY, refreshGmailMailboxes } from "@/lib/gmail-connection";

export function GmailBulk({ mailboxes, mailboxError, mailboxesLoading, refreshMailboxes }: { mailboxes: MailboxDetail[]; mailboxError: unknown; mailboxesLoading: boolean; refreshMailboxes: () => Promise<unknown> }) {
  const { t } = useTranslation("mail");
  const client = useQueryClient();
  const canvas = useCanvasPortal();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<"recipients" | "compose">("recipients");
  const [mailbox, setMailbox] = useState("");
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<string | null>(null);
  const refreshSenders = useMutation({ mutationFn: async () => {
    const connections = await refreshGmailMailboxes();
    client.setQueryData(MAIL_ACCESS_CONNECTIONS_KEY, connections);
    await refreshMailboxes();
  } });
  const recipients = useQuery({ queryKey: [...BULK_KEY, "recipients"], queryFn: listBulkRecipients, enabled: open, retry: false });
  const batches = useQuery({ queryKey: [...BULK_KEY, "batches"], queryFn: listBulk, enabled: open, refetchInterval: open ? 2000 : false, retry: false });
  const choices = useMemo(() => uniqueRecipients(recipients.data ?? []), [recipients.data]);
  const visible = choices.filter(r => `${r.email} ${r.source}`.toLowerCase().includes(filter.toLowerCase()));
  const gmail = mailboxes.filter(m => m.provider === "gmail" && m.available && m.capabilities.includes("send"));
  const gmailAccounts = mailboxes.filter(m => m.provider === "gmail");
  const sendersLoading = mailboxesLoading || refreshSenders.isPending;
  const sendersError = mailboxError || refreshSenders.error;
  const mailboxId = mailbox ? gmail.find(m => m.mailbox_ref_id === mailbox)?.mailbox_ref_id ?? "" : gmail[0]?.mailbox_ref_id ?? "";
  const senderUnavailable = sendersLoading || Boolean(sendersError) || !mailboxId;
  const prepare = useMutation({ mutationFn: prepareBulk, onSuccess: async batch => { await client.invalidateQueries({ queryKey: BULK_KEY }); setPreview(batch.id); } });
  const control = useMutation({ mutationFn: ({ id, action }: { id: string; action: "confirm" | "pause" | "resume" | "cancel" }) => controlBulk(id, action), onSuccess: async () => { setPreview(null); await client.invalidateQueries({ queryKey: BULK_KEY }); } });
  const draft = batches.data?.find(b => b.id === preview);
  const readback = useMutation({ mutationFn: readbackBulk, onSuccess: async () => { await client.invalidateQueries({ queryKey: BULK_KEY }); } });
  const busy = prepare.isPending || control.isPending || readback.isPending;
  return <>
    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b bg-card px-5 py-3">
      <p className="max-w-prose text-sm text-muted-foreground">{t("bulk.description")}</p>
      <Button onClick={() => { refreshSenders.mutate(); setOpen(true); }}><Send className="size-4" aria-hidden />{t("bulk.open")}</Button>
    </div>
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent container={canvas} className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader><DialogTitle>{t("bulk.open")}</DialogTitle><DialogDescription>{t("bulk.description")}</DialogDescription></DialogHeader>
        <ol className="flex flex-wrap gap-x-6 gap-y-2 border-b pb-4 text-sm" aria-label={t("bulk.stepsLabel")}>
          {(["recipients", "compose", "confirm"] as const).map((item, index) => <li key={item} aria-current={(draft ? item === "confirm" : item === step) ? "step" : undefined} className={(draft ? item === "confirm" : item === step) ? "font-semibold text-foreground" : "text-muted-foreground"}>{index + 1}. {t(`bulk.steps.${item}`)}</li>)}
        </ol>
        {Boolean(sendersError) && <p role="alert" className="text-destructive">{t("bulk.senderReadFailed")}</p>}
        {(recipients.isError || batches.isError || prepare.isError || control.isError || readback.isError) && <p role="alert" className="text-destructive">{String(recipients.error || batches.error || prepare.error || control.error || readback.error)}</p>}
        {draft ? <section className="space-y-3">
          <h3 className="font-medium">{t("bulk.preview")}</h3>
          <p>{gmail.find(m => m.mailbox_ref_id === draft.mailbox_ref_id)?.email_address}</p>
          <p className="font-medium">{draft.subject}</p><pre className="whitespace-pre-wrap font-sans text-sm">{draft.body}</pre>
          <ul className="max-h-48 overflow-auto text-sm">{draft.items.map(i => <li key={i.id} className="border-b py-2"><strong>{i.recipient}</strong><p className="whitespace-pre-wrap text-muted-foreground">{i.source}</p>{i.detail && <p className="text-destructive">{i.detail}</p>}</li>)}</ul>
          <p>{t("bulk.confirmHint", { count: draft.items.length })}</p>
          <Button disabled={busy || sendersLoading || Boolean(sendersError) || !gmail.some(m => m.mailbox_ref_id === draft.mailbox_ref_id)} onClick={() => control.mutate({ id: draft.id, action: "confirm" })}>{t("bulk.confirm")}</Button>
          <Button variant="ghost" disabled={busy} onClick={() => setPreview(null)}>{t("common.back")}</Button>
        </section> : <section className="space-y-4">
          {step === "recipients" ? <>
          <label className="block text-sm">{t("common.sender")}<select className="mt-1 w-full rounded border bg-background p-2" value={mailboxId} disabled={sendersLoading || Boolean(sendersError) || !gmailAccounts.length} onChange={e => setMailbox(e.target.value)}>{!mailboxId && <option value="" disabled>{t("common.selectMailbox")}</option>}{gmailAccounts.map(m => <option key={m.mailbox_ref_id} value={m.mailbox_ref_id} disabled={!gmail.some(sender => sender.mailbox_ref_id === m.mailbox_ref_id)}>{m.email_address}{!gmail.some(sender => sender.mailbox_ref_id === m.mailbox_ref_id) ? ` · ${t("mailbox.unavailable")}` : ""}</option>)}</select></label>
          {sendersLoading && <p role="status">{t("common.loading")}</p>}
          {!sendersLoading && !sendersError && !gmail.length && <p>{t(gmailAccounts.length ? "bulk.senderUnavailable" : "bulk.connect")}</p>}
          {(Boolean(sendersError) || !mailboxId) && <Button variant="outline" disabled={sendersLoading} onClick={() => refreshSenders.mutate()}>{t("bulk.reloadSenders")}</Button>}
          <Input aria-label={t("bulk.search")} placeholder={t("bulk.search")} value={filter} onChange={e => setFilter(e.target.value)} />
          <p className="text-xs text-muted-foreground">{t("bulk.limit")}</p>
          <div className="flex gap-2"><Button size="sm" variant="outline" onClick={() => setSelected(new Set(visible.slice(0,100).map(r => r.email)))}>{t("bulk.select")}</Button><Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>{t("bulk.clear")}</Button><span>{t("bulk.selected", { count: selected.size })}</span></div>
          <div className="max-h-48 overflow-auto rounded border">{visible.map(r => <label key={r.email} className="flex gap-2 border-b p-2 text-sm"><Checkbox checked={selected.has(r.email)} disabled={!selected.has(r.email) && selected.size >= 100} onCheckedChange={checked => setSelected(previous => { const next = new Set(previous); if (checked) next.add(r.email); else next.delete(r.email); return next; })} /><span>{r.email}<span className="block whitespace-pre-wrap text-xs text-muted-foreground">{r.source}</span></span></label>)}{recipients.isLoading ? t("common.loading") : !visible.length && <p className="p-2">{t("bulk.empty")}</p>}</div>
          <Button disabled={senderUnavailable || !selected.size || recipients.isLoading || recipients.isError} onClick={() => setStep("compose")}>{t("bulk.nextCompose")}</Button>
          </> : <>
          <p className="text-sm text-muted-foreground">{gmail.find(m => m.mailbox_ref_id === mailboxId)?.email_address} · {t("bulk.selected", { count: selected.size })}</p>
          <label className="block space-y-2 text-sm font-medium"><span>{t("common.subject")}</span><Input placeholder={t("common.subject")} value={subject} maxLength={998} onChange={e => setSubject(e.target.value)} /></label>
          <label className="block space-y-2 text-sm font-medium"><span>{t("bulk.body")}</span><Textarea placeholder={t("bulk.body")} value={body} maxLength={100000} rows={8} onChange={e => setBody(e.target.value)} /></label>
          <div className="flex flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={busy} onClick={() => setStep("recipients")}>{t("bulk.backRecipients")}</Button>
          <Button disabled={busy || senderUnavailable || !selected.size || !subject.trim() || !body.trim()} onClick={() => prepare.mutate({ mailbox_ref_id: mailboxId, subject, body, recipients: choices.filter(r => selected.has(r.email)) })}>{t("bulk.preview")}</Button>
          </div></>}
        </section>}
        <section className="space-y-3 border-t pt-3"><h3 className="font-medium">{t("bulk.history")}</h3>{batches.data?.map(batch => <details key={batch.id} className="rounded border p-3" open={batch.state === "running"}>
          <summary className="cursor-pointer">{batch.subject} · {t(`bulk.state.${batch.state}`)} · {batch.items.filter(i => i.state === "sent").length}/{batch.items.length}</summary>
          {batch.items.some(item => ["accepted", "unknown"].includes(item.state) && item.provider_message_id) && <Button size="sm" variant="outline" disabled={busy} onClick={() => readback.mutate(batch.id)}>{t("bulk.readback")}</Button>}
          <div className="my-2 flex gap-2">{batch.state === "draft" && <Button size="sm" variant="outline" onClick={() => setPreview(batch.id)}>{t("bulk.preview")}</Button>}{batch.state === "running" && <Button size="sm" disabled={busy} onClick={() => control.mutate({ id: batch.id, action: "pause" })}>{t("bulk.pause")}</Button>}{batch.state === "paused" && <Button size="sm" disabled={busy} onClick={() => control.mutate({ id: batch.id, action: "resume" })}>{t("bulk.resume")}</Button>}{["draft","running","paused"].includes(batch.state) && <Button size="sm" variant="outline" disabled={busy} onClick={() => control.mutate({ id: batch.id, action: "cancel" })}>{t("bulk.cancel")}</Button>}</div>
          <ul className="max-h-60 overflow-auto text-sm">{batch.items.map(item => <li key={item.id} className="border-t py-2">{item.recipient} · {t(`bulk.state.${item.state}`)}{item.provider_message_id && <code className="block text-xs">{item.provider_message_id}</code>}{item.detail && <p className="break-words text-xs text-muted-foreground">{item.detail}</p>}</li>)}</ul>
        </details>)}</section>
      </DialogContent>
    </Dialog>
  </>;
}
