/**
 * 收件人选择器（mail feature 内部件）：一个地址行输入框，既能直接输入邮箱手动发送，
 * 也能按名称 / handle 搜索 Creator；选中 Creator 后把 to、creatorId、contactRouteId
 * 一起交给写信窗口，这样模板渲染能自动填内置变量，草稿与发送也关联到 Creator。
 *
 * 接法：整行包在 cmdk 的 Command 里（shouldFilter 关掉，候选由 runtime 查询给出），
 * 输入框用 Command.Input 的 asChild 套在共享 Input 上——视觉与另外两行一致
 * （ADDRESS_INPUT_CLASS），键盘导航与 combobox aria 由 cmdk 提供；建议面板是
 * 一个锚在整行上的 Popover，打开时不抢焦点，焦点始终留在输入框（面板上 mousedown 也不夺焦）。
 *
 * 已知限制：cmdk 给输入框固定写 aria-controls 指向列表 id，而列表只在面板打开时渲染，
 * 收起时该引用悬空；cmdk 不暴露覆盖入口，接受。
 */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Command as CommandPrimitive } from "cmdk";
import { ArrowLeft, AtSign, Loader2, User, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { ADDRESS_INPUT_CLASS } from "@/components/mail/composer-body-editor";
import {
  creatorEmailChoices,
  looksLikeEmail,
  recipientSuggestionState,
  type CreatorEmailChoice,
  type RecipientSelection,
} from "@/components/mail/recipient-picker-model";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandGroup,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import { Input } from "@/components/ui/input";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import type { CreatorSummary } from "@/lib/creator";
import { creatorDetailQueryOptions, creatorsQueryOptions } from "@/lib/runtime-query";

export const RECIPIENT_INPUT_ID = "mail-compose-to";
const SEARCH_DEBOUNCE_MS = 200;
/** chip 刻意比相邻两行（text-base）小一号：药丸密度，行高仍由 AddressRow 的 min-h-12 兜底对齐。 */
const CHIP_CLASS = "inline-flex min-w-0 max-w-full items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-sm";

export type RecipientPickerProps = {
  to: string;
  creatorId: string | null;
  contactRouteId: string | null;
  /** 回复：收件人由线程决定，不可改 */
  locked: boolean;
  disabled: boolean;
  /** 回复且无 creator 时的占位（「由原始邮件 Reply-To 自动确定」） */
  lockedPlaceholder?: string;
  container?: HTMLElement | null;
  /** 手动输入 */
  onToChange: (to: string) => void;
  onCreatorSelect: (selection: RecipientSelection) => void;
  /** 解除关联，保留 to 文本 */
  onCreatorClear: () => void;
  /** 发送快捷键 */
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
};

/** 多邮箱 Creator 的第二步：列出邮箱让用户选。 */
type EmailStep = { creator: CreatorSummary; choices: CreatorEmailChoice[] };

export function RecipientPicker({
  to,
  creatorId,
  locked,
  disabled,
  lockedPlaceholder,
  container,
  onToChange,
  onCreatorSelect,
  onCreatorClear,
  onKeyDown,
}: RecipientPickerProps) {
  if (creatorId) {
    return (
      <CreatorChip
        creatorId={creatorId}
        to={to}
        clearable={!locked}
        disabled={disabled}
        onClear={onCreatorClear}
      />
    );
  }
  if (locked) {
    return (
      <Input
        id={RECIPIENT_INPUT_ID}
        value={to}
        readOnly
        disabled={disabled}
        placeholder={lockedPlaceholder}
        className={ADDRESS_INPUT_CLASS}
        onKeyDown={onKeyDown}
      />
    );
  }
  return (
    <RecipientSearch
      to={to}
      disabled={disabled}
      container={container}
      onToChange={onToChange}
      onCreatorSelect={onCreatorSelect}
      onKeyDown={onKeyDown}
    />
  );
}

/** 已关联 Creator：显示名来自 Creator 详情缓存（选中时已 fetch，回复时 Mail 页已读过），未到时先显示邮箱。 */
function CreatorChip({
  creatorId,
  to,
  clearable,
  disabled,
  onClear,
}: {
  creatorId: string;
  to: string;
  clearable: boolean;
  disabled: boolean;
  onClear: () => void;
}) {
  const { t } = useTranslation("mail");
  const detail = useQuery(creatorDetailQueryOptions(creatorId, true));
  const displayName = detail.data?.creator.display_name ?? null;
  return (
    <div role="group" aria-label={t("recipientPicker.groupLabel")} className="flex min-w-0 items-center py-1">
      <span className={CHIP_CLASS}>
        <User aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{displayName ?? to}</span>
        {displayName && <span className="truncate text-muted-foreground">{`<${to}>`}</span>}
        {clearable && (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("recipientPicker.unlinkCreator", { name: displayName ?? to })}
            disabled={disabled}
            className="-mr-1 size-5"
            onClick={onClear}
          >
            <X aria-hidden="true" />
          </Button>
        )}
      </span>
    </div>
  );
}

function RecipientSearch({
  to,
  disabled,
  container,
  onToChange,
  onCreatorSelect,
  onKeyDown,
}: {
  to: string;
  disabled: boolean;
  container?: HTMLElement | null;
  onToChange: (to: string) => void;
  onCreatorSelect: (selection: RecipientSelection) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
}) {
  const { t } = useTranslation("mail");
  const queryClient = useQueryClient();
  /** 锚点行：Radix 只对 Trigger 特判「点自己不算 outside」，Anchor 没有，得自己挡。 */
  const anchorRef = useRef<HTMLDivElement>(null);
  // 面板只在用户输入或按 ArrowDown 后打开；输入清空即关闭（没有可搜的内容）。
  const [panelRequested, setPanelRequested] = useState(false);
  const [emailStep, setEmailStep] = useState<EmailStep | null>(null);
  const [resolvingCreatorId, setResolvingCreatorId] = useState<string | null>(null);
  const query = to.trim();
  const open = panelRequested && query.length > 0 && !disabled;

  const [debouncedQuery, setDebouncedQuery] = useState(query);
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedQuery(query), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [query]);

  // 只用 creator 列表族既有的 key（runtime-query 是 owner），搜索词进 key 的 criteria。
  const creators = useQuery(creatorsQueryOptions(open && debouncedQuery.length > 0, { query: debouncedQuery }));
  const suggestions = recipientSuggestionState({
    query,
    debouncedQuery,
    loading: creators.isLoading,
    error: creators.error,
    creators: creators.data?.items,
  });

  const closePanel = () => {
    setPanelRequested(false);
    setEmailStep(null);
  };
  const choose = (creator: CreatorSummary, choice: CreatorEmailChoice) => {
    closePanel();
    onCreatorSelect({
      creatorId: creator.id,
      contactRouteId: choice.contactRouteId,
      to: choice.email,
      displayName: creator.display_name,
    });
  };
  const selectCreator = async (creator: CreatorSummary) => {
    setResolvingCreatorId(creator.id);
    try {
      const record = await queryClient.fetchQuery(creatorDetailQueryOptions(creator.id, true));
      const choices = creatorEmailChoices(record);
      if (choices.length === 0) {
        toast.error(t("recipientPicker.noEmail"));
        return;
      }
      if (choices.length === 1) {
        choose(creator, choices[0]);
        return;
      }
      setEmailStep({ creator, choices });
    } catch (error) {
      toast.error(t("recipientPicker.readCreatorFailed"), { description: String(error) });
    } finally {
      setResolvingCreatorId(null);
    }
  };

  const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented) return;
    // 输入法组合期间方向键是在选候选词，不碰（cmdk 根自己也跳过 isComposing）。
    if (event.nativeEvent.isComposing || event.keyCode === 229) return;
    // Home/End 任何时候都留给输入框光标：cmdk 根会 preventDefault 并跳列表首尾。
    if (event.key === "Home" || event.key === "End") {
      event.stopPropagation();
      return;
    }
    if (open) return;
    // 面板关着时不让 cmdk 根接管方向键/Enter；ArrowDown 在有内容时重新打开面板（进入列表）。
    if (event.key === "ArrowDown" && query.length > 0) {
      event.preventDefault();
      setPanelRequested(true);
      return;
    }
    if (["ArrowUp", "ArrowDown", "Enter"].includes(event.key)) {
      event.stopPropagation();
    }
  };
  const liveMessage = suggestions.kind === "loading"
    ? t("recipientPicker.searching")
    : suggestions.kind === "empty"
      ? t("recipientPicker.noMatches")
      : suggestions.kind === "error"
        ? suggestions.message ?? t("recipientPicker.searchFailed")
        : "";

  return (
    <Popover open={open} onOpenChange={(next) => (next ? setPanelRequested(true) : closePanel())}>
      <Command
        shouldFilter={false}
        loop
        vimBindings={false}
        className="h-auto overflow-visible rounded-none bg-transparent"
      >
        <PopoverAnchor asChild>
          <div ref={anchorRef} className="flex min-w-0 items-center">
            {/* 常驻的 live region：随加载态一起挂载/卸载的 aria-live 不会被播报。 */}
            <div aria-live="polite" className="sr-only">{open ? liveMessage : ""}</div>
            <CommandPrimitive.Input asChild value={to} onValueChange={(value) => {
              onToChange(value);
              setEmailStep(null);
              setPanelRequested(true);
            }}>
              {/* cmdk 会给 aria-labelledby 指向它自己（未渲染）的 label；这一行由 AddressRow 的 Label 通过 htmlFor 命名。 */}
              <Input
                id={RECIPIENT_INPUT_ID}
                aria-labelledby={undefined}
                aria-expanded={open}
                disabled={disabled}
                placeholder={t("recipientPicker.searchPlaceholder")}
                className={ADDRESS_INPUT_CLASS}
                onKeyDown={handleInputKeyDown}
              />
            </CommandPrimitive.Input>
          </div>
        </PopoverAnchor>
        <PopoverContent
          align="start"
          container={container}
          className="w-[var(--radix-popover-trigger-width)] p-0"
          onOpenAutoFocus={(event) => event.preventDefault()}
          onCloseAutoFocus={(event) => event.preventDefault()}
          // 点候选时不把焦点从输入框挪到面板：面板随后关闭/卸载会让焦点掉到 body，
          // Tab 就会逃出模态抽屉（combobox 常规做法；没有 Trigger 可供 Radix 回聚焦）。
          onMouseDown={(event) => event.preventDefault()}
          onInteractOutside={(event) => {
            if (anchorRef.current?.contains(event.target as Node)) event.preventDefault();
          }}
        >
          <CommandList label={t("recipientPicker.candidatesLabel")}>
            {emailStep ? (
              <CommandGroup heading={t("recipientPicker.creatorEmails", { name: emailStep.creator.display_name })}>
                {emailStep.choices.map((choice) => (
                  <CommandItem
                    key={choice.contactRouteId}
                    value={`route:${choice.contactRouteId}`}
                    onSelect={() => choose(emailStep.creator, choice)}
                  >
                    <AtSign aria-hidden="true" />
                    <span className="truncate">{choice.email}</span>
                  </CommandItem>
                ))}
                <CommandItem value="back" onSelect={() => setEmailStep(null)}>
                  <ArrowLeft aria-hidden="true" />
                  {t("common.back")}
                </CommandItem>
              </CommandGroup>
            ) : (
              <>
                {looksLikeEmail(to) && (
                  <>
                    <CommandGroup>
                      <CommandItem value="manual-email" onSelect={closePanel}>
                        <AtSign aria-hidden="true" />
                        <span className="truncate">{t("recipientPicker.useEmail", { email: query })}</span>
                      </CommandItem>
                    </CommandGroup>
                    <CommandSeparator />
                  </>
                )}
                <CommandGroup heading="Creator">
                  {suggestions.kind === "loading" && (
                    <div className="flex items-center justify-center py-4">
                      <Loader2 aria-hidden="true" className="size-4 animate-spin text-muted-foreground" />
                    </div>
                  )}
                  {suggestions.kind === "error" && (
                    <div className="px-2 py-3 text-sm text-destructive">
                      {suggestions.message ?? t("recipientPicker.searchFailed")}
                    </div>
                  )}
                  {suggestions.kind === "empty" && (
                    <div className="px-2 py-3 text-sm text-muted-foreground">{t("recipientPicker.noMatches")}</div>
                  )}
                  {suggestions.kind === "ready" && suggestions.creators.map((creator) => (
                    <CommandItem
                      key={creator.id}
                      value={`creator:${creator.id}`}
                      disabled={resolvingCreatorId !== null}
                      onSelect={() => void selectCreator(creator)}
                    >
                      <User aria-hidden="true" />
                      <span className="min-w-0 flex-1 truncate">{creator.display_name}</span>
                      {creator.primary_handle && (
                        <span className="truncate text-xs text-muted-foreground">{creator.primary_handle}</span>
                      )}
                      {resolvingCreatorId === creator.id && <Loader2 aria-hidden="true" className="size-3.5 animate-spin" />}
                    </CommandItem>
                  ))}
                </CommandGroup>
              </>
            )}
          </CommandList>
        </PopoverContent>
      </Command>
    </Popover>
  );
}
