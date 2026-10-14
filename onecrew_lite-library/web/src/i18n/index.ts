import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import zhCN from './locales/zh-CN.json';
import en from './locales/en.json';

const storageKey = 'onecrew.language';
const saved = window.localStorage.getItem(storageKey);
const language = saved === 'en' ? 'en' : 'zh-CN';
i18n.use(initReactI18next).init({
  resources: { 'zh-CN': { translation: zhCN }, en: { translation: en } },
  lng: language,
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});
document.documentElement.lang = language === 'en' ? 'en' : 'zh-CN';
export const setLanguage = (next: 'zh-CN' | 'en') => {
  window.localStorage.setItem(storageKey, next);
  document.documentElement.lang = next === 'en' ? 'en' : 'zh-CN';
  void i18n.changeLanguage(next);
};
export default i18n;
