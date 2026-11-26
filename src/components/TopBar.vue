<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppStore } from "@/stores/app";
import type { IconName } from "@/icons";
import AppIcon from "./AppIcon.vue";
import LedDot from "./LedDot.vue";

const store = useAppStore();
const emit = defineEmits<{ (e: "refresh"): void; (e: "open-settings"): void }>();

const themeIcon = computed<IconName>(() =>
  store.theme === "dark" ? "moon" : store.theme === "light" ? "sun" : "system"
);
const themeLabel = computed(
  () => ({ system: "跟随系统", dark: "深色", light: "浅色" } as const)[store.theme]
);

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
/** 浏览器预览（mock 数据）下没有 Tauri 运行时，窗口句柄为空 */
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
/** 顶栏任意空白处按住左键都能拖动窗口；按钮/输入等交互元素不触发 */
function onTopbarMouseDown(e: MouseEvent) {
  if (e.button !== 0 || !win) return;
  const t = e.target as HTMLElement | null;
  if (t?.closest("button, a, input, textarea, select, [data-no-drag]")) return;
  void win.startDragging();
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
        <span>运行</span>
      </button>
      <button
        class="chip"
        :class="{ on: store.filter === 'stopped' }"
        @click="store.setFilter('stopped')"
      >
        <LedDot state="stopped" :size="7" :pulse="false" />
        <span class="mono num">{{ store.counts.stopped }}</span>
        <span>停止</span>
      </button>
      <button
        class="chip"
        :class="{ on: store.filter === 'error' }"
        @click="store.setFilter('error')"
      >
        <LedDot state="error" :size="7" :pulse="false" />
        <span class="mono num">{{ store.counts.error }}</span>
        <span>错误</span>
      </button>
    </div>

    <div class="rack" title="项目状态灯带">
      <span
        v-for="p in store.projects"
        :key="p.id"
        class="rack-led"
        :data-state="p.state"
        :title="p.name"
      />
    </div>

    <div class="spacer" />

    <button class="server" title="全局服务器与版本设置" @click="emit('open-settings')">
      <LedDot state="running" :size="7" :pulse="false" />
      <span class="label">服务器</span>
      <span class="mono addr">{{ store.server.addr }}:{{ store.server.port }}</span>
      <span class="tag">{{ store.server.protocol }}</span>
      <span class="tag">tls</span>
    </button>

    <button class="btn icon" title="刷新状态" @click="emit('refresh')">
      <AppIcon name="refresh" :size="14" />
    </button>
    <button class="btn icon" :title="'主题：' + themeLabel" @click="store.cycleTheme()">
      <AppIcon :name="themeIcon" :size="14" />
    </button>
    <button class="btn icon" title="设置" @click="emit('open-settings')">
      <AppIcon name="settings" :size="14" />
    </button>

    <div class="win-ctl" data-no-drag>
      <button class="btn icon wbtn" title="最小化" aria-label="最小化" :disabled="!isTauri" @click="minimizeWindow">
        <AppIcon name="minimize" :size="13" />
      </button>
      <button
        class="btn icon wbtn"
        :title="maximized ? '还原' : '最大化'"
        :aria-label="maximized ? '还原' : '最大化'"
        :disabled="!isTauri"
        @click="toggleMaximize"
      >
        <AppIcon :name="maximized ? 'restore' : 'maximize'" :size="12" />
      </button>
      <button class="btn icon wbtn close" title="关闭" aria-label="关闭" :disabled="!isTauri" @click="closeWindow">
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
  transition: color 0.15s ease, border-color 0.15s ease, background-color 0.15s ease;
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
  width: 4px;
  height: 10px;
  border-radius: 1px;
  background: var(--text-faint);
  opacity: 0.4;
}
.rack-led[data-state="running"] {
  background: var(--accent);
  opacity: 1;
  box-shadow: 0 0 5px 0 color-mix(in srgb, var(--accent) 55%, transparent);
}
.rack-led[data-state="starting"] {
  background: var(--amber);
  opacity: 1;
}
.rack-led[data-state="error"] {
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
  transition: border-color 0.15s ease, color 0.15s ease;
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
