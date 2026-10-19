/**
 * 邮件正文编辑器（mail feature 内部件，不是共享原语）：写信窗口与模板编辑器
 * 共用同一段「动作条 + 格式栏 + Markdown textarea + 预览」。
 *
 * 只管正文文本与 tab/格式栏的视图交互，不管 html：调用方拿 onChange 上报的
 * 文本自己派生 html（写信窗口还要处理模板 html 原样保留、纯文本模式等状态），
 * 再把用于预览的 html 传回来。
 *
 * 动作条「图片」弹出的表单插入的是内联 HTML `<img … width="N%">` 而不是 `![]()`：
 * Markdown 图片语法没有尺寸，而收件人客户端没有我们预览里的 `img{max-width:100%}`，
 * 原图会撑爆邮件版面（标记本身见 markdown-edit.ts 的 imageMarkup）。
 */
import {
  forwardRef,
  useId,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { flushSync } from "react-dom";
import { ALargeSmall, ChevronDown, ChevronUp, Image as ImageIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ComposerBodyPreview } from "@/components/mail/composer-body-preview";
import {
  ComposerFormatToolbar,
  type FormatToolbarCommand,
} from "@/components/mail/composer-format-toolbar";
import {
  applyMarkdownCommand,
  IMAGE_WIDTH_PRESETS,
  insertImageAtSelection,
  insertTextAtSelection,
  isHttpUrl,
  isImageWidthPercentInRange,
  normalizeImageWidthPercent,
  type ImageInsertOptions,
  type MarkdownCommand,
  type MarkdownEdit,
} from "@/components/mail/markdown-edit";
import type { EmailBodyStyle } from "@/components/mail/markdown-email";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export type ComposerBodyTab = "edit" | "preview";

/** 调用方通过 ref 往正文插入文本（模板编辑器「插入变量」），走与格式命令相同的撤销栈路径。 */
export type ComposerBodyEditorHandle = {
  insertText: (text: string) => void;
  focus: () => void;
};

/** 地址行里的 Input 去掉边框，整行的 focus-within 背景就是它的焦点态。 */
export const ADDRESS_INPUT_CLASS = "h-auto rounded-none border-0 px-0 py-1 text-base shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent";

export type ComposerBodyEditorProps = {
  /** textarea 的 id（写信窗口用它做初始焦点） */
  id: string;
  value: string;
  /** 只上报文本；html 由调用方派生 */
  onChange: (text: string) => void;
  /** 预览用的 html；null 时预览按纯文本显示 */
  html: string | null;
  disabled: boolean;
  plainText: boolean;
  placeholder: string;
  formatExpanded: boolean;
  onFormatExpandedChange: (expanded: boolean) => void;
  tab: ComposerBodyTab;
  onTabChange: (tab: ComposerBodyTab) => void;
  /** 省略时格式栏不显示字体/字号菜单（模板编辑器不保存 html，字体字号无处可存） */
  bodyStyle?: EmailBodyStyle;
  onBodyStyleChange?: (style: EmailBodyStyle) => void;
  /** 写信窗口用来把字体字号显示在编辑区 */
  editorStyle?: CSSProperties;
  onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  /** 动作条里「图片」之后、编辑/预览 Tabs 之前的额外按钮 */
  actionBarExtras?: ReactNode;
  /** 格式栏与正文之间的内容（写信窗口：模板横幅与提示） */
  aboveBody?: ReactNode;
  /** 正文列底部的内容（写信窗口：历史邮件） */
  belowBody?: ReactNode;
  /** 各段落共用的水平留白类名 */
  inset: string;
  formatToolbarId?: string;
  /** 弹层的 Portal 目标（写信 Sheet / 模板 Dialog 各自的 container），与 DialogContent 同一契约 */
  container?: HTMLElement | null;
};

/** 图片表单的状态：宽度要么是一个预设（null = 原始尺寸），要么是「自定义 %」里输入的字符串。 */
type ImageFormState = {
  url: string;
  alt: string;
  preset: number | null;
  custom: string;
};

const EMPTY_IMAGE_FORM: ImageFormState = { url: "", alt: "", preset: null, custom: "" };

/** 自定义框里有内容就以它为准，否则用预设（null = 原始）。 */
function imageFormOptions(form: ImageFormState): ImageInsertOptions {
  const custom = form.custom.trim();
  return {
    url: form.url,
    alt: form.alt,
    widthPercent: custom ? normalizeImageWidthPercent(custom) : form.preset,
  };
}

export const ComposerBodyEditor = forwardRef<ComposerBodyEditorHandle, ComposerBodyEditorProps>(function ComposerBodyEditor({
  id,
  value,
  onChange,
  html,
  disabled,
  plainText,
  placeholder,
  formatExpanded,
  onFormatExpandedChange,
  tab,
  onTabChange,
  bodyStyle,
  onBodyStyleChange,
  editorStyle,
  onKeyDown,
  actionBarExtras,
  aboveBody,
  belowBody,
  inset,
  formatToolbarId = `${id}-format-toolbar`,
  container,
}, ref) {
  const { t } = useTranslation("mail");
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const imageTitleId = useId();
  // 图片表单：受控 open，每次打开都重置为空/原始。点「图片」时焦点会进表单，
  // textarea 的选区在 blur 后仍可读，但保险起见点击当下就把它记下，插入时按这段选区替换。
  const [imageOpen, setImageOpen] = useState(false);
  const [imageForm, setImageForm] = useState<ImageFormState>(EMPTY_IMAGE_FORM);
  const imageSelection = useRef<{ start: number; end: number } | null>(null);
  // 插入后焦点已回到正文 textarea，不让 Radix 在关闭时把焦点归还给「图片」按钮；
  // Esc / 点外部取消仍回到按钮（与格式栏「更多格式」的 commandChosen 同一写法）。
  const imageInserted = useRef(false);

  /**
   * 让正文 textarea 可操作并聚焦：预览 tab 下它所在的面板是 display:none，聚焦和
   * execCommand 都不会生效，先同步切回编辑 tab，DOM 更新完再操作。
   */
  const focusBodyEditor = (): HTMLTextAreaElement | null => {
    const textarea = bodyRef.current;
    if (!textarea || disabled || plainText) return null;
    if (tab !== "edit") {
      flushSync(() => onTabChange("edit"));
    }
    textarea.focus();
    return textarea;
  };
  /**
   * 把一次编辑落到 textarea 上：只替换 [start, end) 这一段。
   * document.execCommand 已被标为弃用，但 WKWebView 与 WebView2 都仍然支持，
   * 而且它是唯一能保住 textarea 原生撤销栈（Cmd/Ctrl+Z、工具栏撤销）的方式：
   * 直接 setState 回写整段文本会让浏览器把这次修改当成外部赋值，撤销栈被清空。
   * 它会派发 input 事件，于是走正常的 onChange 路径。
   */
  const applyEdit = (textarea: HTMLTextAreaElement, edit: MarkdownEdit) => {
    textarea.setSelectionRange(edit.start, edit.end);
    let applied = false;
    try {
      applied = edit.insert.length > 0
        ? document.execCommand("insertText", false, edit.insert)
        : edit.start !== edit.end && document.execCommand("delete");
    } catch {
      applied = false;
    }
    if (!applied && (edit.insert.length > 0 || edit.start !== edit.end)) {
      // 不支持 execCommand 的环境：整体回写（会丢撤销栈，但结果一致）。
      onChange(edit.text);
    }
    requestAnimationFrame(() => {
      textarea.setSelectionRange(edit.selectionStart, edit.selectionEnd);
    });
  };
  const runMarkdownCommand = (command: MarkdownCommand) => {
    const textarea = focusBodyEditor();
    if (!textarea) return;
    applyEdit(textarea, applyMarkdownCommand(
      { text: textarea.value, start: textarea.selectionStart, end: textarea.selectionEnd },
      command,
      {
        bold: t("editor.insertDefaults.bold"),
        italic: t("editor.insertDefaults.italic"),
        underline: t("editor.insertDefaults.underline"),
        strike: t("editor.insertDefaults.strike"),
        link: t("editor.insertDefaults.link"),
        image: t("editor.insertDefaults.image"),
        tableHeader1: t("editor.insertDefaults.tableHeader1"),
        tableHeader2: t("editor.insertDefaults.tableHeader2"),
      },
    ));
  };
  useImperativeHandle(ref, () => ({
    insertText: (text) => {
      const textarea = focusBodyEditor();
      if (!textarea) return;
      applyEdit(textarea, insertTextAtSelection(
        { text: textarea.value, start: textarea.selectionStart, end: textarea.selectionEnd },
        text,
      ));
    },
    focus: () => {
      focusBodyEditor();
    },
  }));
  const openImageForm = () => {
    const textarea = bodyRef.current;
    const start = textarea?.selectionStart ?? value.length;
    const end = textarea?.selectionEnd ?? value.length;
    imageSelection.current = { start, end };
    // 与旧的 Markdown 图片命令一致：选中的一段文字（单行）作为描述预填，其余字段重置。
    const selected = value.slice(Math.min(start, end), Math.max(start, end)).trim();
    setImageForm({ ...EMPTY_IMAGE_FORM, alt: selected.includes("\n") ? "" : selected });
  };
  const insertImage = () => {
    const options = imageFormOptions(imageForm);
    if (!isHttpUrl(options.url) || !isImageWidthPercentInRange(imageForm.custom)) return;
    const textarea = focusBodyEditor();
    if (!textarea) return;
    const selection = imageSelection.current ?? { start: textarea.value.length, end: textarea.value.length };
    imageInserted.current = true;
    setImageOpen(false);
    applyEdit(textarea, insertImageAtSelection({ text: textarea.value, ...selection }, options));
  };
  const imageUrlValid = isHttpUrl(imageForm.url);
  const imageUrlError = imageForm.url.trim() !== "" && !imageUrlValid;
  const imageCustomActive = imageForm.custom.trim() !== "";
  // 越界的自定义 % 不静默截断：表单里给出提示并禁用「插入」（normalizeImageWidthPercent 的夹取只是兜底）。
  const imageCustomError = !isImageWidthPercentInRange(imageForm.custom);
  const imageFieldId = (name: string) => `${id}-image-${name}`;
  const handleToolbarCommand = (command: FormatToolbarCommand) => {
    if (command === "undo" || command === "redo") {
      // 撤销/重做走浏览器原生撤销栈（execCommand 的取舍见 runMarkdownCommand）。
      if (!focusBodyEditor()) return;
      try {
        document.execCommand(command);
      } catch {
        // 不支持 execCommand 的环境：没有可用的撤销栈，静默即可。
      }
      return;
    }
    runMarkdownCommand(command);
  };

  return (
    // Tabs 根包住动作条与正文列：TabsTrigger 的 aria-controls 要指向真实存在的
    // TabsContent（tabpanel），而 TabsContent 必须在 Tabs 根之内。根本身只是父列里
    // 的一段 flex 列，不改变各段的留白与伸缩。
    <Tabs
      value={tab}
      onValueChange={(next) => onTabChange(next === "preview" ? "preview" : "edit")}
      className="flex min-h-0 flex-1 flex-col gap-0"
    >
      {/* 文字标签在 @xl(576px) 以下收起：@md 档位标签全显 + 留白升级会把 Tabs 顶出右边界；overflow 只是兜底 */}
      <div className={cn("flex h-11 min-w-0 shrink-0 items-center gap-1 overflow-x-auto border-b [scrollbar-width:thin]", inset)}>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-expanded={formatExpanded}
          aria-controls={formatToolbarId}
          disabled={plainText || disabled}
          className="data-[state=open]:bg-muted"
          onClick={() => onFormatExpandedChange(!formatExpanded)}
        >
          <ALargeSmall aria-hidden="true" />
          <span className="@max-xl:sr-only">{t("editor.format")}</span>
          {formatExpanded
            ? <ChevronUp aria-hidden="true" className="size-3 opacity-70" />
            : <ChevronDown aria-hidden="true" className="size-3 opacity-70" />}
        </Button>
        <Separator orientation="vertical" className="mx-1 h-5" />
        {/* 图片是一段内联 HTML，纯文本模式下没有意义。 */}
        <Popover
          open={imageOpen}
          onOpenChange={(open) => {
            if (open) openImageForm();
            setImageOpen(open);
          }}
        >
          <PopoverTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label={t("editor.image")}
              disabled={disabled || plainText}
              className="data-[state=open]:bg-muted"
            >
              <ImageIcon aria-hidden="true" />
              <span className="@max-xl:sr-only">{t("editor.image")}</span>
            </Button>
          </PopoverTrigger>
          <PopoverContent
            align="start"
            container={container}
            aria-labelledby={imageTitleId}
            className="w-80"
            onCloseAutoFocus={(event) => {
              if (!imageInserted.current) return;
              imageInserted.current = false;
              event.preventDefault();
            }}
          >
            {/* noValidate：链接校验走 isHttpUrl 与禁用的「插入」按钮，不让浏览器原生气泡插手。 */}
            <form
              noValidate
              className="flex flex-col gap-3"
              onSubmit={(event) => {
                event.preventDefault();
                insertImage();
              }}
            >
              <p id={imageTitleId} className="text-sm font-medium">{t("editor.imageDialog.title")}</p>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor={imageFieldId("url")}>{t("editor.imageDialog.url")}</Label>
                <Input
                  id={imageFieldId("url")}
                  type="url"
                  required
                  autoComplete="off"
                  placeholder="https://…"
                  value={imageForm.url}
                  aria-invalid={imageUrlError ? true : undefined}
                  aria-describedby={imageUrlError ? imageFieldId("url-error") : undefined}
                  onChange={(event) => setImageForm((current) => ({ ...current, url: event.target.value }))}
                />
                {/* role="alert"：提示随错误出现才挂载，挂载即播报；常驻 aria-live 反而要先在 DOM 里才生效。 */}
                {imageUrlError && (
                  <p id={imageFieldId("url-error")} role="alert" className="text-xs text-destructive">{t("editor.imageDialog.invalidUrl")}</p>
                )}
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor={imageFieldId("alt")}>{t("editor.imageDialog.alt")}</Label>
                <Input
                  id={imageFieldId("alt")}
                  autoComplete="off"
                  placeholder={t("editor.imageDialog.altPlaceholder")}
                  value={imageForm.alt}
                  onChange={(event) => setImageForm((current) => ({ ...current, alt: event.target.value }))}
                />
              </div>
              <div className="flex flex-col gap-1.5">
                {/* 只是 group 的 aria-labelledby 目标，不指向任何控件，所以不用 <label>。 */}
                <p id={imageFieldId("width-label")} className="text-sm font-medium leading-none">{t("editor.imageDialog.size")}</p>
                {/* 预设是一组互斥开关：aria-pressed 表示当前选中；自定义框里一有内容就取代预设。 */}
                <div role="group" aria-labelledby={imageFieldId("width-label")} className="flex flex-wrap gap-1">
                  <Button
                    type="button"
                    size="sm"
                    variant={!imageCustomActive && imageForm.preset === null ? "secondary" : "outline"}
                    aria-pressed={!imageCustomActive && imageForm.preset === null}
                    onClick={() => setImageForm((current) => ({ ...current, preset: null, custom: "" }))}
                  >
                    {t("editor.imageDialog.original")}
                  </Button>
                  {IMAGE_WIDTH_PRESETS.map((preset) => {
                    const active = !imageCustomActive && imageForm.preset === preset;
                    return (
                      <Button
                        key={preset}
                        type="button"
                        size="sm"
                        variant={active ? "secondary" : "outline"}
                        aria-pressed={active}
                        className="tabular-nums"
                        onClick={() => setImageForm((current) => ({ ...current, preset, custom: "" }))}
                      >
                        {preset}%
                      </Button>
                    );
                  })}
                </div>
                <div className="flex items-center gap-2">
                  <Label htmlFor={imageFieldId("custom-width")} className="shrink-0 font-normal text-muted-foreground">{t("editor.imageDialog.customPercent")}</Label>
                  <Input
                    id={imageFieldId("custom-width")}
                    type="number"
                    inputMode="numeric"
                    min={1}
                    max={100}
                    step={1}
                    placeholder="1–100"
                    className="h-8 w-24 tabular-nums"
                    value={imageForm.custom}
                    aria-invalid={imageCustomError ? true : undefined}
                    aria-describedby={imageCustomError ? imageFieldId("custom-width-error") : undefined}
                    onChange={(event) => setImageForm((current) => ({ ...current, custom: event.target.value, preset: null }))}
                  />
                </div>
                {imageCustomError && (
                  <p id={imageFieldId("custom-width-error")} role="alert" className="text-xs text-destructive">{t("editor.imageDialog.invalidPercent")}</p>
                )}
              </div>
              <div className="flex justify-end gap-2">
                <Button type="button" variant="outline" size="sm" onClick={() => setImageOpen(false)}>{t("common.cancel")}</Button>
                <Button type="submit" size="sm" disabled={!imageUrlValid || imageCustomError}>{t("editor.imageDialog.insert")}</Button>
              </div>
            </form>
          </PopoverContent>
        </Popover>
        {actionBarExtras}
        <TabsList className="ml-auto shrink-0">
          <TabsTrigger value="edit">{t("common.edit")}</TabsTrigger>
          <TabsTrigger value="preview">{t("common.preview")}</TabsTrigger>
        </TabsList>
      </div>

      {formatExpanded && !plainText && (
        <div id={formatToolbarId} className={cn("shrink-0 pt-1.5", inset)}>
          <ComposerFormatToolbar
            disabled={disabled}
            bodyStyle={bodyStyle}
            onBodyStyleChange={onBodyStyleChange}
            onCommand={handleToolbarCommand}
            container={container}
          />
        </div>
      )}

      {aboveBody}

      <div className={cn("flex min-h-0 flex-1 flex-col", inset)}>
        {/* 预览时编辑面板只是隐藏（hidden 属性）而不卸载：forceMount 保住 textarea，
            卸载会清空浏览器原生撤销栈。面板本身不占 Tab 位，焦点直接落在 textarea 上。 */}
        <TabsContent
          value="edit"
          forceMount
          hidden={tab !== "edit"}
          tabIndex={-1}
          className="flex min-h-0 flex-1 flex-col"
        >
          <Textarea
            id={id}
            ref={bodyRef}
            value={value}
            disabled={disabled}
            placeholder={placeholder}
            className="min-h-0 flex-1 resize-none rounded-none border-0 px-0 py-3 text-base shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent"
            style={editorStyle}
            onKeyDown={onKeyDown}
            onChange={(event) => onChange(event.target.value)}
          />
        </TabsContent>
        <TabsContent value="preview" className="flex min-h-0 flex-1 flex-col">
          <ComposerBodyPreview
            html={html}
            text={value}
            plainText={plainText}
            className="py-4"
          />
        </TabsContent>
        {belowBody}
      </div>
    </Tabs>
  );
});

/**
 * 发件人 / 收件人 / 主题 这类行式字段：左侧固定宽度的标签，右侧内容撑满，
 * 行底一条分隔线（飞书写信窗的形态）。写信窗口与模板编辑器共用。
 */
export function AddressRow({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-12 items-center border-b transition-colors focus-within:bg-muted/30">
      {htmlFor ? (
        <Label htmlFor={htmlFor} className="w-20 shrink-0 text-base font-normal text-muted-foreground">{label}</Label>
      ) : (
        <span className="w-20 shrink-0 text-base text-muted-foreground">{label}</span>
      )}
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}
