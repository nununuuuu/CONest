import { createI18n } from 'vue-i18n';
import zhCN from './locales/zh-CN.json';
import zhTW from './locales/zh-TW.json';
import enUS from './locales/en-US.json';
import jaJP from './locales/ja-JP.json';

export const locales = [
  { id: 'zh-TW', name: '繁體中文 zh-TW', msg: zhTW },
  { id: 'zh-CN', name: '简体中文 🇨🇳', msg: zhCN },
  { id: 'en-US', name: 'English 🇺🇸', msg: enUS },
  { id: 'ja-JP', name: '日本語 🇯🇵', msg: jaJP },
];

export default createI18n({
  legacy: false,
  fallbackLocale: 'zh-TW',
  messages: Object.fromEntries(locales.map((loc) => [loc.id, loc.msg])),
});
