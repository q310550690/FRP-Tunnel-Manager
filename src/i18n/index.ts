import { computed, ref } from "vue";
import type { App } from "vue";
import type { Language } from "element-plus/es/locale";
import zhCn from "element-plus/es/locale/lang/zh-cn";
import zhTw from "element-plus/es/locale/lang/zh-tw";
import en from "element-plus/es/locale/lang/en";
import ja from "element-plus/es/locale/lang/ja";
import ko from "element-plus/es/locale/lang/ko";
import es from "element-plus/es/locale/lang/es";

import zhCNMessages from "./locales/zh-CN.json";
import zhTWMessages from "./locales/zh-TW.json";
import enUSMessages from "./locales/en-US.json";
import jaJPMessages from "./locales/ja-JP.json";
import koKRMessages from "./locales/ko-KR.json";
import esESMessages from "./locales/es-ES.json";

export type LocaleCode = "zh-CN" | "zh-TW" | "en-US" | "ja-JP" | "ko-KR" | "es-ES";

export interface LocaleOption {
  code: LocaleCode;
  /** 语言名称使用该语言自己的写法，切换时不需要翻译 */
  label: string;
}

export const LOCALES: LocaleOption[] = [
  { code: "zh-CN", label: "简体中文" },
  { code: "zh-TW", label: "繁體中文" },
  { code: "en-US", label: "English" },
  { code: "ja-JP", label: "日本語" },
  { code: "ko-KR", label: "한국어" },
  { code: "es-ES", label: "Español" },
];

const MESSAGES: Record<LocaleCode, Record<string, string>> = {
  "zh-CN": zhCNMessages as Record<string, string>,
  "zh-TW": zhTWMessages as Record<string, string>,
  "en-US": enUSMessages as Record<string, string>,
  "ja-JP": jaJPMessages as Record<string, string>,
  "ko-KR": koKRMessages as Record<string, string>,
  "es-ES": esESMessages as Record<string, string>,
};

const ELEMENT_LOCALES: Record<LocaleCode, Language> = {
  "zh-CN": zhCn,
  "zh-TW": zhTw,
  "en-US": en,
  "ja-JP": ja,
  "ko-KR": ko,
  "es-ES": es,
};

const FALLBACK_LOCALE: LocaleCode = "zh-CN";

function normalizeLocale(raw?: string | null): LocaleCode | null {
  if (!raw) return null;
  const lower = raw.toLowerCase().replace("_", "-");
  if (lower.startsWith("zh")) {
    return lower.includes("tw") || lower.includes("hk") || lower.includes("hant") ? "zh-TW" : "zh-CN";
  }
  if (lower.startsWith("en")) return "en-US";
  if (lower.startsWith("ja")) return "ja-JP";
  if (lower.startsWith("ko")) return "ko-KR";
  if (lower.startsWith("es")) return "es-ES";
  return null;
}

/** 首次启动跟随系统语言；系统语言不在支持列表时回退英文 */
export function detectLocale(): LocaleCode {
  if (typeof navigator === "undefined") return FALLBACK_LOCALE;
  const candidates = Array.isArray(navigator.languages) && navigator.languages.length
    ? navigator.languages
    : [navigator.language];
  for (const candidate of candidates) {
    const match = normalizeLocale(candidate);
    if (match) return match;
  }
  return "en-US";
}

export const locale = ref<LocaleCode>(detectLocale());

export function isLocaleCode(value: unknown): value is LocaleCode {
  return typeof value === "string" && LOCALES.some((item) => item.code === value);
}

export function setLocale(value: string): void {
  const next = normalizeLocale(value) ?? FALLBACK_LOCALE;
  locale.value = next;
  if (typeof document !== "undefined") document.documentElement.lang = next;
}

export function t(key: string, params?: Record<string, string | number>): string {
  const dict = MESSAGES[locale.value] ?? MESSAGES[FALLBACK_LOCALE];
  let text = dict[key] ?? MESSAGES[FALLBACK_LOCALE][key] ?? key;
  if (params) {
    text = text.replace(/\{(\w+)\}/g, (match, name: string) =>
      Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match,
    );
  }
  return text;
}

export const elementLocale = computed(() => ELEMENT_LOCALES[locale.value] ?? ELEMENT_LOCALES[FALLBACK_LOCALE]);

export function useI18n() {
  return { locale, locales: LOCALES, t, setLocale, elementLocale, detectLocale };
}

export function initI18n(app: App): void {
  app.config.globalProperties.$t = t;
  setLocale(locale.value);
}
