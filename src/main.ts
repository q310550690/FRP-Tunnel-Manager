import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

import "element-plus/theme-chalk/dark/css-vars.css";
import "element-plus/es/components/message-box/style/css";
import "element-plus/es/components/message/style/css";
import "./styles/tokens.css";
import { initI18n } from "./i18n";

const app = createApp(App);
initI18n(app);
app.use(createPinia()).mount("#app");

/** 启动层（index.html 内联）最短展示时长：避免应用秒开时闪一下就没 */
const SPLASH_MIN_VISIBLE_MS = 520;
/** 淡出时长，需略大于 .ftp-splash 的 300ms opacity 过渡 */
const SPLASH_FADE_MS = 340;

// 等 Vue 首帧真正画出来再开始收尾；浏览器预览 / 热更新重跑时启动层可能已移除
requestAnimationFrame(() => {
  const splash = document.querySelector<HTMLElement>(".ftp-splash");
  if (!splash) return;

  const startedAt = (window as Window & { __FTPM_SPLASH_AT__?: number }).__FTPM_SPLASH_AT__;
  const shownFor = typeof startedAt === "number" ? performance.now() - startedAt : 0;
  const remaining = Math.max(0, SPLASH_MIN_VISIBLE_MS - shownFor);

  window.setTimeout(() => {
    if (!splash.isConnected) return;
    splash.classList.add("boot-hide");
    window.setTimeout(() => splash.remove(), SPLASH_FADE_MS);
  }, remaining);
});
