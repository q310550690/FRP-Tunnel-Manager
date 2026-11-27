<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppStore } from "@/stores/app";
import type { IconName } from "@/icons";
import AppIcon from "./AppIcon.vue";
import LedDot from "./LedDot.vue";
import { t as translate } from "@/i18n";

const store = useAppStore();
const emit = defineEmits<{ (e: "refresh"): void; (e: "open-settings"): void }>();

const themeIcon = computed<IconName>(() =>
  store.theme === "dark" ? "moon" : store.theme === "light" ? "sun" : "system"
);
const themeLabel = computed(() =>
  translate(({ system: "跟随系统", dark: "深色", light: "浅色" } as const)[store.theme])
);

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
/** 浏览器预览下没有 Tauri 运行时，窗口句柄为空 */
const win = isTauri ? getCurrentWindow() : null;
const maximized = ref(false);

async function syncMaximized() {
  if (!win) return;
  try {
    maximized.value = await win.isMaximized();
  } catch {
    /* 浏览器预览下忽略 */
  }
}
function minimizeWindow() {
  void win?.minimize();
}
function toggleMaximize() {
  if (!win) return;
  void win.toggleMaximize().then(syncMaximized);
}
function closeWindow() {
  void win?.close();
}
/** 点击状态灯带：清掉筛选/搜索并定位到对应项目，右侧详情随之打开 */
function openProject(id: string) {
  store.query = "";
  if (store.filter !== "all") store.filter = "all";
  store.select(id);
}
/** 顶栏任意空白处按住左键都能拖动窗口；按钮/输入等交互元素不触发 */
let dragging = false;
async function onTopbarMouseDown(e: MouseEvent) {
  if (e.button !== 0 || !win) return;
  const t = e.target as HTMLElement | null;
  if (!t || t.closest("button, a, input, textarea, select, [data-no-drag]")) return;
  // 双击交给窗口自带的「最大化/还原」，不在这里拖
  if (e.detail > 1) return;
  e.preventDefault();
  if (dragging) return;
  dragging = true;
  try {
    // Windows 上窗口未激活时，首次按下只用于激活窗口，直接 startDragging 会被忽略，
    // 用户看到的就是「有时候拖不动」。先聚焦再拖。
    const focused = await win.isFocused().catch(() => true);
    if (!focused) await win.setFocus().catch(() => undefined);
    await win.startDragging();
  } catch (err) {
    console.warn("窗口拖动失败：", err);
  } finally {
    dragging = false;
  }
}
let unlisten: (() => void) | undefined;
onMounted(async () => {
  if (!win) return;
  await syncMaximized();
  unlisten = await win.onResized(() => {
    void syncMaximized();
  });
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <header class="topbar" @mousedown="onTopbarMouseDown">
    <div class="brand">
      <div class="mark" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none">
          <path
            d="M4.5 18.6v-7.1a7.5 7.5 0 0 1 15 0v7.1"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <circle class="mark-led" cx="12" cy="15.4" r="1.55" />
        </svg>
      </div>
      <div class="brand-text">
        <strong>FRP Tunnel Manager</strong>
      </div>
    </div>

    <div class="stats">
      <button
        class="chip"
        :class="{ on: store.filter === 'running' }"
        @click="store.setFilter('running')"
      >
        <LedDot state="running" :size="7" :pulse="false" />
        <span class="mono num">{{ store.counts.running }}</span>
        <span>{{ $t("运行") }}</span>
      </button>
      <button
        class="chip"
        :class="{ on: store.filter === 'stopped' }"
        @click="store.setFilter('stopped')"
      >
        <LedDot state="stopped" :size="7" :pulse="false" />
        <span class="mono num">{{ store.counts.stopped }}</span>
        <span>{{ $t("停止") }}</span>
      </button>
      <button
        class="chip"
        :class="{ on: store.filter === 'error' }"
        @click="store.setFilter('error')"
      >
        <LedDot state="error" :size="7" :pulse="false" />
        <span class="mono num">{{ store.counts.error }}</span>
        <span>{{ $t("错误") }}</span>
      </button>
    </div>

    <div v-if="store.projects.length" class="rack" :title="$t('项目状态灯带，点击定位项目')">
      <button
        v-for="p in store.projects"
        :key="p.id"
        type="button"
        class="rack-led"
        :data-state="p.state"
        :title="p.proxy.name || p.name"
        :aria-label="$t('定位到项目 {name}', { name: p.proxy.name || p.name })"
        @click="openProject(p.id)"
      >
        <span class="rack-bar" aria-hidden="true" />
      </button>
    </div>

    <div class="spacer" />

    <button
      class="server"
      :class="{ unset: !store.server.addr }"
      :title="store.server.addr ? $t('全局服务器与版本设置') : $t('还没有配置服务器，点击去设置')"
      @click="emit('open-settings')"
    >
      <LedDot :state="store.server.addr ? 'running' : 'stopped'" :size="7" :pulse="false" />
      <span class="label">{{ $t("服务器") }}</span>
      <template v-if="store.server.addr">
        <span class="mono addr">{{ store.server.addr }}:{{ store.server.port }}</span>
        <span class="tag">{{ store.server.protocol }}</span>
        <span class="tag">tls</span>
      </template>
      <template v-else>
        <span class="mono addr">{{ $t("未配置") }}</span>
        <span class="tag err">{{ $t("去设置") }}</span>
      </template>
    </button>

    <button class="btn icon" :title="$t('刷新状态')" @click="emit('refresh')">
      <AppIcon name="refresh" :size="14" />
    </button>
    <button class="btn icon" :title="$t('主题：{name}', { name: themeLabel })" @click="store.cycleTheme()">
      <AppIcon :name="themeIcon" :size="14" />
    </button>
    <button class="btn icon" :title="$t('设置')" @click="emit('open-settings')">
      <AppIcon name="settings" :size="14" />
    </button>

    <div class="win-ctl" data-no-drag>
      <button class="btn icon wbtn" :title="$t('最小化')" :aria-label="$t('最小化')" :disabled="!isTauri" @click="minimizeWindow">
        <AppIcon name="minimize" :size="13" />
      </button>
      <button
        class="btn icon wbtn"
        :title="maximized ? $t('还原') : $t('最大化')"
        :aria-label="maximized ? $t('还原') : $t('最大化')"
        :disabled="!isTauri"
        @click="toggleMaximize"
      >
        <AppIcon :name="maximized ? 'restore' : 'maximize'" :size="12" />
      </button>
      <button class="btn icon wbtn close" :title="$t('关闭')" :aria-label="$t('关闭')" :disabled="!isTauri" @click="closeWindow">
        <AppIcon name="close" :size="13" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  height: var(--h-topbar);
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  padding: 0 var(--sp-2);
  background: var(--bg-panel);
  border-bottom: 1px solid var(--line);
  flex: none;
  user-select: none;
}

/* brand 在左；版本号已按用户要求移除 */
.brand {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  padding-right: var(--sp-1);
  flex: none;
}
/* brand 左侧标识：机架面板（外框 + 两道散热槽 + 状态灯） */
.mark {
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  color: var(--text-dim);
  flex: none;
}
.mark svg {
  display: block;
}
.mark-led {
  fill: var(--accent);
  filter: drop-shadow(0 0 2.5px var(--accent));
}
.brand-text {
  display: flex;
  align-items: baseline;
  gap: var(--sp-1);
}
.brand-text strong {
  font-size: var(--fs-3);
  font-weight: 600;
  letter-spacing: 0.02em;
}

.stats {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  margin-left: var(--sp-1);
}
.chip {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: 26px;
  padding: 0 var(--sp-1);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius);
  color: var(--text-dim);
  font-family: inherit;
  font-size: var(--fs-2);
  line-height: 1;
  cursor: pointer;
  transition: color 0.1s ease, border-color 0.1s ease, background-color 0.1s ease;
}
.chip:hover {
  color: var(--text);
  background: var(--bg-hover);
}
.chip.on {
  border-color: var(--line-accent);
  background: var(--bg-hover);
  color: var(--text);
}
.chip .num {
  color: var(--text);
  font-weight: 600;
}

.rack {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: 20px;
  padding: 0 var(--sp-2);
  margin-left: var(--sp-1);
  border-left: 1px solid var(--line);
  border-right: 1px solid var(--line);
}
.rack-led {
  appearance: none;
  width: 10px;
  height: 20px;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
}
.rack-bar {
  width: 4px;
  height: 10px;
  border-radius: 1px;
  background: var(--text-faint);
  opacity: 0.4;
  transform-origin: center;
  transition: transform 0.1s ease, opacity 0.1s ease;
}
.rack-led:hover .rack-bar {
  transform: scaleY(1.1);
  opacity: 1;
}
.rack-led:focus-visible {
  outline: 1px solid var(--accent);
  outline-offset: 2px;
}
.rack-led[data-state="running"] .rack-bar {
  background: var(--accent);
  opacity: 1;
  box-shadow: 0 0 5px 0 color-mix(in srgb, var(--accent) 55%, transparent);
}
.rack-led[data-state="starting"] .rack-bar {
  background: var(--amber);
  opacity: 1;
}
.rack-led[data-state="error"] .rack-bar {
  background: var(--red);
  opacity: 1;
  box-shadow: 0 0 5px 0 color-mix(in srgb, var(--red) 55%, transparent);
}

.spacer {
  flex: 1;
}

.server {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: 26px;
  padding: 0 var(--sp-2);
  background: transparent;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  color: var(--text-dim);
  font-family: inherit;
  cursor: pointer;
  font-size: var(--fs-2);
  transition: border-color 0.1s ease, color 0.1s ease;
}
.server:hover {
  border-color: var(--line-accent);
  color: var(--text);
}
.server .addr {
  color: var(--text);
  font-size: var(--fs-2);
}
.win-ctl {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: 26px;
  margin-left: var(--sp-1);
  padding-left: 12px;
  border-left: 1px solid var(--line);
}
/* 窗口按钮沿用 .btn.icon 的尺寸/描边/hover，只把关闭键的 hover 换成红色 */
.win-ctl .wbtn.close:hover:not(:disabled) {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 55%, var(--line));
  background: color-mix(in srgb, var(--red) 8%, transparent);
}
</style>
