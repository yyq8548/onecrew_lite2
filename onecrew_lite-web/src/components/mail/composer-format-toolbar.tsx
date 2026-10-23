/**
 * 写邮件正文的格式栏（mail feature 内部件，不是共享原语）。
 *
 * 上游（shadcn registry / `@shadcn/react`）没有「Markdown 工具栏」原语，所以这里
 * 用既有原语组合：容器是 radix Toolbar（`radix-ui` 导出的 Toolbar），拿到
 * role="toolbar" 与左右方向键的 roving focus（整条工具栏只占一个 Tab 位）；
 * 按钮、下拉、气泡都是 components/ui 里的 shadcn 原语。真正的 Markdown 编辑
 * 动作在 markdown-edit.ts 的纯函数里，这里只上报 command，由 composer 拿
 * textarea 的选区去执行（撤销/重做同样交给 composer 走浏览器原生撤销栈）。
 *
 * 这些按钮都是即时动作（点一下改一次文本），不是「当前处于加粗态」的开关，
 * 所以不用 Toggle/aria-pressed；字体、字号是整封邮件的一个设置，改的是
 * EmailBodyStyle，最终包在派生 html 的外层 div style 上。
 */
import { useRef, useState } from "react";
import type { ComponentType, MouseEvent, SVGProps } from "react";
import type { TFunction } from "i18next";
import {
  Bold,
  ChevronDown,
  Ellipsis,
  Italic,
  Link2,
  List,
  ListOrdered,
  Quote,
  Redo2,
  Strikethrough,
  Table2,
  Underline,
  Undo2,
} from "lucide-react";
import { Toolbar as ToolbarPrimitive } from "radix-ui";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { MarkdownCommand } from "@/components/mail/markdown-edit";
import {
  EMAIL_FONT_OPTIONS,
  EMAIL_FONT_SIZES,
  emailFontOptionForFamily,
  type EmailBodyStyle,
} from "@/components/mail/markdown-email";
import { cn } from "@/lib/utils";

export type FormatToolbarCommand = MarkdownCommand | "undo" | "redo";

export const FORMAT_TOOLBAR_COMMAND_KEYS = {
  undo: "toolbar.commands.undo",
  redo: "toolbar.commands.redo",
  bold: "toolbar.commands.bold",
  italic: "toolbar.commands.italic",
  underline: "toolbar.commands.underline",
  strike: "toolbar.commands.strike",
  ol: "toolbar.commands.ol",
  ul: "toolbar.commands.ul",
  quote: "toolbar.commands.quote",
  link: "toolbar.commands.link",
  image: "toolbar.commands.image",
  table: "toolbar.commands.table",
} as const satisfies Record<FormatToolbarCommand, string>;

/** 字号下拉用字符串做 RadioGroup value；null（跟随默认）映射成固定的哨兵值。 */
export const FONT_SIZE_DEFAULT_VALUE = "default";

export function fontSizeMenuValue(fontSize: number | null): string {
  return fontSize === null ? FONT_SIZE_DEFAULT_VALUE : String(fontSize);
}

export function fontSizeFromMenuValue(value: string): number | null {
  if (value === FONT_SIZE_DEFAULT_VALUE) return null;
  const size = Number(value);
  return Number.isFinite(size) && size > 0 ? size : null;
}

export function fontSizeTriggerLabel(fontSize: number | null, defaultLabel: string): string {
  return fontSize === null ? defaultLabel : String(fontSize);
}

/** 字体下拉的 RadioGroup value 用 EMAIL_FONT_OPTIONS 的 value；未知 family 回落到系统默认。 */
export function fontFamilyFromMenuValue(value: string): string | null {
  return EMAIL_FONT_OPTIONS.find((option) => option.value === value)?.family ?? null;
}

const TOOLTIP_DELAY = 160;

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

function fontOptionLabel(value: string, t: TFunction<"mail">): string {
  switch (value) {
    case "system": return t("toolbar.fonts.system");
    case "heiti": return t("toolbar.fonts.heiti");
    case "songti": return t("toolbar.fonts.songti");
    case "nsimsun": return t("toolbar.fonts.nsimsun");
    case "kaiti": return t("toolbar.fonts.kaiti");
    case "lishu": return t("toolbar.fonts.lishu");
    case "youyuan": return t("toolbar.fonts.youyuan");
    case "yahei": return t("toolbar.fonts.yahei");
    case "arial": return t("toolbar.fonts.arial");
    default: return value;
  }
}

/**
 * 工具栏容器是 bg-muted，而 theme.css 里 --accent 与 --muted 同值，ghost 按钮
 * 默认的 hover:bg-accent 在这里看不出变化：悬停/打开态改用 bg-background 提亮。
 */
const TOOLBAR_BUTTON_CLASS = "hover:bg-background data-[state=open]:bg-background dark:hover:bg-background";

/**
 * 按下工具栏按钮时不把焦点从 textarea 抢走：composer 需要 textarea 保持焦点与
 * 选区才能用 execCommand("insertText") 做最小替换（保住原生撤销栈）。
 * 只拦鼠标按下，键盘操作（roving focus + Enter/Space）不受影响；代价是
 * preventDefault 让 radix 跳过 onItemFocus，鼠标点击不会移动 roving tab stop。
 */
function keepEditorFocus(event: MouseEvent<HTMLButtonElement>) {
  event.preventDefault();
}

function CommandButton({
  command,
  icon: Icon,
  disabled,
  onCommand,
}: {
  command: FormatToolbarCommand;
  icon: IconComponent;
  disabled: boolean;
  onCommand: (command: FormatToolbarCommand) => void;
}) {
  const { t } = useTranslation("mail");
  const label = t(FORMAT_TOOLBAR_COMMAND_KEYS[command]);
  return (
    <Tooltip delayDuration={TOOLTIP_DELAY}>
      <TooltipTrigger asChild>
        {/* disabled 也要给 Toolbar.Button：asChild 合并拿不到子元素的 disabled，roving focus 才会跳过禁用项 */}
        <ToolbarPrimitive.Button asChild disabled={disabled}>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            disabled={disabled}
            className={TOOLBAR_BUTTON_CLASS}
            onMouseDown={keepEditorFocus}
            onClick={() => onCommand(command)}
          >
            <Icon />
          </Button>
        </ToolbarPrimitive.Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

function FontFamilyMenu({
  disabled,
  bodyStyle,
  onBodyStyleChange,
}: {
  disabled: boolean;
  bodyStyle: EmailBodyStyle;
  onBodyStyleChange: (style: EmailBodyStyle) => void;
}) {
  const { t } = useTranslation("mail");
  const current = emailFontOptionForFamily(bodyStyle.fontFamily);
  const currentLabel = fontOptionLabel(current.value, t);
  return (
    <DropdownMenu>
      <Tooltip delayDuration={TOOLTIP_DELAY}>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <ToolbarPrimitive.Button asChild disabled={disabled}>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={t("toolbar.fontCurrent", { font: currentLabel })}
                disabled={disabled}
                className={cn("min-w-24 justify-between px-2 font-normal", TOOLBAR_BUTTON_CLASS)}
              >
                <span className="truncate">{currentLabel}</span>
                <ChevronDown className="text-muted-foreground" aria-hidden="true" />
              </Button>
            </ToolbarPrimitive.Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t("toolbar.font")}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start">
        <DropdownMenuRadioGroup
          value={current.value}
          onValueChange={(value) => onBodyStyleChange({ ...bodyStyle, fontFamily: fontFamilyFromMenuValue(value) })}
        >
          {EMAIL_FONT_OPTIONS.map((option) => (
            <DropdownMenuRadioItem
              key={option.value}
              value={option.value}
              style={option.family ? { fontFamily: option.family } : undefined}
            >
              {fontOptionLabel(option.value, t)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function FontSizeMenu({
  disabled,
  bodyStyle,
  onBodyStyleChange,
}: {
  disabled: boolean;
  bodyStyle: EmailBodyStyle;
  onBodyStyleChange: (style: EmailBodyStyle) => void;
}) {
  const { t } = useTranslation("mail");
  const label = fontSizeTriggerLabel(bodyStyle.fontSize, t("toolbar.defaultSize"));
  return (
    <DropdownMenu>
      <Tooltip delayDuration={TOOLTIP_DELAY}>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <ToolbarPrimitive.Button asChild disabled={disabled}>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={t("toolbar.sizeCurrent", { size: label })}
                disabled={disabled}
                className={cn("min-w-14 justify-between px-2 font-normal tabular-nums", TOOLBAR_BUTTON_CLASS)}
              >
                <span className="truncate">{label}</span>
                <ChevronDown className="text-muted-foreground" aria-hidden="true" />
              </Button>
            </ToolbarPrimitive.Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t("toolbar.size")}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="min-w-24">
        <DropdownMenuRadioGroup
          value={fontSizeMenuValue(bodyStyle.fontSize)}
          onValueChange={(value) => onBodyStyleChange({ ...bodyStyle, fontSize: fontSizeFromMenuValue(value) })}
        >
          <DropdownMenuRadioItem value={FONT_SIZE_DEFAULT_VALUE}>{t("toolbar.defaultSize")}</DropdownMenuRadioItem>
          {EMAIL_FONT_SIZES.map((size) => (
            <DropdownMenuRadioItem key={size} value={String(size)} className="tabular-nums">
              {size}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const MORE_COMMANDS: ReadonlyArray<{ command: MarkdownCommand; icon: IconComponent }> = [
  { command: "link", icon: Link2 },
  { command: "table", icon: Table2 },
];

function MoreMenu({
  disabled,
  onCommand,
  container,
}: {
  disabled: boolean;
  onCommand: (command: FormatToolbarCommand) => void;
  container?: HTMLElement | null;
}) {
  const { t } = useTranslation("mail");
  // 受控 open：点了「链接/表格」后要主动收起，再把动作交给 composer。
  const [open, setOpen] = useState(false);
  // 选了动作时焦点已由 composer 移回正文 textarea（见 runMarkdownCommand），
  // 不让 Radix 在关闭时把焦点归还给触发器；Esc 取消仍回到触发器。
  const commandChosen = useRef(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip delayDuration={TOOLTIP_DELAY}>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <ToolbarPrimitive.Button asChild disabled={disabled}>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={t("toolbar.more")}
                disabled={disabled}
                className={TOOLBAR_BUTTON_CLASS}
              >
                <Ellipsis />
              </Button>
            </ToolbarPrimitive.Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t("toolbar.more")}</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="end"
        container={container}
        className="flex w-36 flex-col gap-0.5 p-1"
        onCloseAutoFocus={(event) => {
          if (!commandChosen.current) return;
          commandChosen.current = false;
          event.preventDefault();
        }}
      >
        {MORE_COMMANDS.map(({ command, icon: Icon }) => (
          <Button
            key={command}
            type="button"
            variant="ghost"
            size="sm"
            className="justify-start font-normal"
            onClick={() => {
              commandChosen.current = true;
              setOpen(false);
              onCommand(command);
            }}
          >
            <Icon className="text-muted-foreground" aria-hidden="true" />
            {t(FORMAT_TOOLBAR_COMMAND_KEYS[command])}
          </Button>
        ))}
      </PopoverContent>
    </Popover>
  );
}

function ToolbarDivider() {
  return <Separator orientation="vertical" className="mx-0.5 h-5" />;
}

export function ComposerFormatToolbar({
  disabled,
  bodyStyle,
  onBodyStyleChange,
  onCommand,
  className,
  container,
}: {
  disabled: boolean;
  /** 字体/字号是整封邮件的设置，写进派生 html；调用方没有地方存它（模板编辑器）时两者都省略，菜单就不渲染 */
  bodyStyle?: EmailBodyStyle;
  onBodyStyleChange?: (style: EmailBodyStyle) => void;
  onCommand: (command: FormatToolbarCommand) => void;
  className?: string;
  /** 「更多格式」弹层的 Portal 目标 */
  container?: HTMLElement | null;
}) {
  const { t } = useTranslation("mail");
  return (
    <ToolbarPrimitive.Root
      aria-label={t("toolbar.label")}
      aria-disabled={disabled || undefined}
      className={cn(
        "flex items-center gap-0.5 overflow-x-auto rounded-lg bg-muted px-1.5 py-0.5 [scrollbar-width:thin]",
        className,
      )}
    >
      <CommandButton command="undo" icon={Undo2} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="redo" icon={Redo2} disabled={disabled} onCommand={onCommand} />
      <ToolbarDivider />
      {bodyStyle && onBodyStyleChange && (
        <>
          <FontFamilyMenu disabled={disabled} bodyStyle={bodyStyle} onBodyStyleChange={onBodyStyleChange} />
          <FontSizeMenu disabled={disabled} bodyStyle={bodyStyle} onBodyStyleChange={onBodyStyleChange} />
          <ToolbarDivider />
        </>
      )}
      <CommandButton command="bold" icon={Bold} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="italic" icon={Italic} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="underline" icon={Underline} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="strike" icon={Strikethrough} disabled={disabled} onCommand={onCommand} />
      <ToolbarDivider />
      <CommandButton command="ol" icon={ListOrdered} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="ul" icon={List} disabled={disabled} onCommand={onCommand} />
      <CommandButton command="quote" icon={Quote} disabled={disabled} onCommand={onCommand} />
      <ToolbarDivider />
      <MoreMenu disabled={disabled} onCommand={onCommand} container={container} />
    </ToolbarPrimitive.Root>
  );
}
