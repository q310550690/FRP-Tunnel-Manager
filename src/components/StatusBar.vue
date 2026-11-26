<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ElMessage, ElMessageBox } from "element-plus";
import { useAppStore } from "@/stores/app";
import AppIcon from "./AppIcon.vue";

const store = useAppStore();

/** 发布仓库：检查更新直接读 GitHub 的 latest release */
const REPO = "q310550690/FRP-Tunnel-Manager";
const LATEST_API = `https://api.github.com/repos/${REPO}/releases/latest`;

interface ReleaseInfo {
  version: string;
  url: string;
  name: string;
  publishedAt: string;
}

/** 软件版本；浏览器预览时取不到 Tauri API，回落到 package.json 里的版本 */
const appVersion = ref("0.1.0");
/** 是否正在请求 GitHub（图标转圈） */
const checking = ref(false);
/** 已检测到的新版本；非空时图标高亮并带红点 */
const update = ref<ReleaseInfo | null>(null);

const hasUpdate = computed(() => update.value !== null);
const updateTip = computed(() =>
  update.value
    ? `发现新版本 v${update.value.version}，点击查看`
    : `检查更新（当前 v${appVersion.value}）`
);

const inTauri = () => "__TAURI_INTERNALS__" in window;

onMounted(async () => {
  try {
    appVersion.value = await getVersion();
  } catch {
    /* 非 Tauri 环境下保持默认值 */
  }
  // 启动后静默检查一次：检测到新版本先弹确认框，用户确认后才打开浏览器
  window.setTimeout(() => void checkUpdate(false), 1500);
});

function parseVersion(v: string): number[] {
  return v
    .replace(/^v/i, "")
    .split("-")[0]
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
}

/** latest 是否比 current 新（只比较主版本号，rc 等后缀忽略） */
function isNewer(latest: string, current: string): boolean {
  const a = parseVersion(latest);
  const b = parseVersion(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}

async function fetchLatest(): Promise<ReleaseInfo> {
  const proxy = store.network.useProxy ? store.network.proxyUrl : null;
  let raw: string;
  if (inTauri()) {
    // 走 Rust 侧请求：沿用「设置 → 网络代理」
    raw = await invoke<string>("http_get_text", { url: LATEST_API, proxy });
  } else {
    const res = await fetch(LATEST_API, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    raw = await res.text();
  }
  const rel = JSON.parse(raw) as {
    tag_name?: string;
    html_url?: string;
    name?: string;
    published_at?: string;
  };
  const version = String(rel.tag_name ?? "").replace(/^v/i, "");
  if (!version || !rel.html_url) throw new Error("GitHub 返回的 Release 数据不完整");
  return {
    version,
    url: rel.html_url,
    name: rel.name ?? "",
    publishedAt: rel.published_at ?? "",
  };
}

function errText(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/404/.test(msg)) return "GitHub 上还没有发布过版本";
  if (/403|rate limit/i.test(msg)) return "GitHub API 请求过于频繁，请稍后再试";
  return `检查更新失败：${msg}`;
}

async function openRelease(url: string) {
  try {
    if (inTauri()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  } catch (e) {
    ElMessage.error(`打开浏览器失败：${errText(e)}`);
  }
}

/** 检测到新版本：先提示，用户点「打开发布页」才跳转 */
let prompting = false;
async function promptUpdate(info: ReleaseInfo) {
  if (prompting) return;
  prompting = true;
  try {
    await ElMessageBox.confirm(
      `发现新版本 v${info.version}（当前 v${appVersion.value}），是否打开 GitHub 发布页查看更新内容？`,
      "发现更新",
      { confirmButtonText: "打开发布页", cancelButtonText: "稍后", type: "info" }
    );
    await openRelease(info.url);
  } catch {
    /* 用户选择「稍后」：保留图标红点，下次点击还能再提示 */
  } finally {
    prompting = false;
  }
}

async function checkUpdate(manual: boolean) {
  if (checking.value) return;
  checking.value = true;
  try {
    const latest = await fetchLatest();
    if (isNewer(latest.version, appVersion.value)) {
      update.value = latest;
      await promptUpdate(latest);
    } else {
      update.value = null;
      if (manual) ElMessage.success(`已是最新版本 v${appVersion.value}`);
    }
  } catch (e) {
    if (manual) ElMessage.warning(errText(e));
  } finally {
    checking.value = false;
  }
}

function stamp() {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}
</script>

<template>
  <footer class="statusbar">
    <span class="item">
      <AppIcon name="layers" :size="11" />
      <span class="label">项目</span>
      <span class="mono">{{ store.projects.length }}</span>
    </span>
    <span class="sep" />
    <span class="item">
      <AppIcon name="folder" :size="11" />
      <span class="mono">./projects</span>
    </span>
    <span class="sep" />
    <span class="item">
      <span class="label">全局默认 FRP 版本</span>
      <span class="mono">{{ store.defaultVersion }}</span>
    </span>
    <span class="sep" />
    <span class="item">
      <span class="label">配置</span>
      <span class="mono">config/app.json</span>
    </span>

    <span class="flex" />

    <span class="item muted">
      <AppIcon name="refresh" :size="11" />
      <span class="mono">最后刷新 {{ stamp() }}</span>
    </span>
    <span class="sep" />
    <span class="item muted">
      <!-- 检测更新：点一下查 GitHub latest release，有新版先确认再打开发布页 -->
      <button
        type="button"
        class="update"
        :class="{ busy: checking, fresh: hasUpdate }"
        :title="updateTip"
        :aria-label="updateTip"
        :disabled="checking"
        @click="checkUpdate(true)"
      >
        <AppIcon :name="checking ? 'refresh' : 'down'" :size="11" />
        <span v-if="hasUpdate" class="dot" />
      </button>
      <span class="label">版本</span>
      <span class="mono">v{{ appVersion }}</span>
    </span>
  </footer>
</template>

<style scoped>
/* 9) 与顶栏去重：这里只留顶栏没有的信息（项目数 / 本地目录 / 全局默认 FRP 版本 / 配置文件 / 刷新时间 / 软件版本） */
.statusbar {
  height: var(--h-statusbar);
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 0 var(--sp-2);
  background: var(--bg-panel);
  border-top: 1px solid var(--line);
  font-size: var(--fs-1);
  color: var(--text-faint);
}
.item {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
}
.label {
  font-size: var(--fs-1);
}
.sep {
  width: 1px;
  height: 10px;
  background: var(--line-strong);
}
.flex {
  flex: 1;
}

/* 检测更新图标：紧挨版本号，发现新版本时变主题色 + 右上角红点 */
.update {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.update:hover:not(:disabled) {
  color: var(--text);
  background: var(--bg-hover);
}
.update.fresh {
  color: var(--accent);
}
.update.busy {
  cursor: progress;
}
.update.busy :deep(.app-icon) {
  animation: update-spin 0.9s linear infinite;
}
.dot {
  position: absolute;
  top: 0;
  right: 0;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--red);
}
@keyframes update-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
