import accessI18n from "./i18n";

export function currentAccessLanguage() {
  return accessI18n.language === "en-US" ? "en-US" : "zh-CN";
}
