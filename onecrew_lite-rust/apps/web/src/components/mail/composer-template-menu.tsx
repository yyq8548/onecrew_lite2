import type { JSX } from "react";
import type { TFunction } from "i18next";
import {
  Check,
  ChevronDown,
  FileText,
  LayoutTemplate,
  Loader2,
  Save,
  Settings,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { TemplateMenuState } from "@/components/mail/composer-model";
import { emailTemplateLoadErrorDescription } from "@/components/mail/template-error-presentation";
import type { EmailTemplateSummary } from "@/lib/creator";

/**
 * 写邮件工具栏里的「模板」下拉：应用模板 / 保存为模板 / 管理模板。
 * 菜单状态（loading / error / empty / ready）由 composer-model 的 templateMenuState 派生，
 * 这里只负责把它渲染成 radix DropdownMenu 项。
 */
export function ComposerTemplateMenu({
  state,
  activeTemplateId,
  disabled,
  saveDisabled = false,
  onApply,
  onSaveAsTemplate,
  onManage,
}: {
  state: TemplateMenuState;
  activeTemplateId: string | null;
  disabled: boolean;
  /** 渲染进行中时禁用「保存为模板」 */
  saveDisabled?: boolean;
  onApply: (template: EmailTemplateSummary) => void;
  onSaveAsTemplate: () => void;
  onManage: () => void;
}): JSX.Element {
  const { t } = useTranslation("mail");
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t("templates.button")}
          className="data-[state=open]:bg-muted"
        >
          <LayoutTemplate aria-hidden="true" />
          {/* 窄画布只留图标：容器查询由工具栏的 @container 祖先提供 */}
          <span className="@max-xl:sr-only">{t("templates.button")}</span>
          <ChevronDown aria-hidden="true" className="size-3 opacity-70" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuLabel className="flex flex-col gap-0.5">
          <span>{t("templates.menu.apply")}</span>
          <span className="text-xs font-normal text-muted-foreground">{t("templates.menu.replaceHint")}</span>
        </DropdownMenuLabel>
        {renderTemplateItems(state, activeTemplateId, onApply, t)}
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={saveDisabled} onSelect={() => onSaveAsTemplate()}>
          <Save aria-hidden="true" />
          {t("templates.menu.saveAs")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onManage()}>
          <Settings aria-hidden="true" />
          {t("templates.menu.manage")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function renderTemplateItems(
  state: TemplateMenuState,
  activeTemplateId: string | null,
  onApply: (template: EmailTemplateSummary) => void,
  t: TFunction<"mail">,
): JSX.Element | JSX.Element[] {
  switch (state.kind) {
    case "loading":
      return (
        <DropdownMenuItem disabled>
          <Loader2 aria-hidden="true" className="animate-spin" />
          {t("templates.menu.loading")}
        </DropdownMenuItem>
      );
    case "error":
      return (
        <DropdownMenuItem disabled>
          <FileText aria-hidden="true" />
          {/* 禁用项 pointer-events-none，原生 title 弹不出来：错误信息直接作为第二行渲染 */}
          <span className="flex min-w-0 flex-col">
            <span>{t("templates.menu.error")}</span>
            <span className="line-clamp-2 text-xs font-normal text-muted-foreground">
              {emailTemplateLoadErrorDescription(state.error, t)}
            </span>
          </span>
        </DropdownMenuItem>
      );
    case "empty":
      return (
        <DropdownMenuItem disabled>
          <FileText aria-hidden="true" />
          {t("templates.menu.empty")}
        </DropdownMenuItem>
      );
    case "ready":
      return state.templates.map((template) => {
        const active = template.id === activeTemplateId;
        return (
          <DropdownMenuItem
            key={template.id}
            aria-current={active ? "true" : undefined}
            onSelect={() => onApply(template)}
          >
            <span className="min-w-0 flex-1 truncate">{template.name}</span>
            {active ? <Check aria-hidden="true" className="text-foreground" /> : null}
          </DropdownMenuItem>
        );
      });
  }
}
