import { useState, type ComponentProps } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useAccessI18n } from "@/lib/access-i18n";

export function PasswordInput(props: ComponentProps<typeof Input>) {
  const [visible, setVisible] = useState(false);
  const { t } = useAccessI18n();
  return <div className="auth-password-field">
    <Input {...props} type={visible ? "text" : "password"} />
    <button type="button" aria-pressed={visible} aria-label={t(visible ? "auth.design.hidePassword" : "auth.design.showPassword")} onClick={() => setVisible(value => !value)}>
      {visible ? <Eye size={22} aria-hidden="true" /> : <EyeOff size={22} aria-hidden="true" />}
    </button>
  </div>;
}
