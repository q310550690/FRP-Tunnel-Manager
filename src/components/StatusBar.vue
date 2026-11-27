<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ElMessage, ElMessageBox } from "element-plus";
import { Github } from "lucide-vue-next";
import { useAppStore, type PendingUpdate } from "@/stores/app";
import AppIcon from "./AppIcon.vue";

const store = useAppStore();

/** 发布仓库：检查更新直接读 GitHub 的 latest release */
const REPO = "q310550690/FRP-Tunnel-Manager";
const REPO_URL = `https://github.com/${REPO}`;
const LATEST_API = `https://api.github.com/repos/${REPO}/releases/latest`;

interface ReleaseAsset {
  name: string;
  url: string;
  size: number;
}

interface ReleaseInfo {
  version: string;
  url: string;
  name: string;
  publishedAt: string;
  assets: ReleaseAsset[];
}

type Phase = "idle" | "checking" | "downloading" | "ready" | "installing" | "error";

/** 软件版本；浏览器预览时取不到 Tauri API，回落到 package.json 里的版本 */
const appVersion = ref("0.1.0");
/** 当前更新流程所处阶段 */
const phase = ref<Phase>("idle");
/** 已检测到的新版本（含资产列表） */
const update = ref<ReleaseInfo | null>(null);
/** 下载进度 0–100 */
const progress = ref(0);
/** 下载失败原因（显示在 title / 版本号位置） */
const downloadError = ref("");

/** 最短转圈时长：请求再快（甚至秒失败）也至少转 0.65s，让点击有可见反馈 */
const MIN_CHECK_SPIN_MS = 650;

let progressUnlisten: UnlistenFn | undefined;
let startupTimer: ReturnType<typeof setTimeout> | undefined;
let stopLoadedWatch: (() => void) | undefined;
let initialized = false;

const busy = computed(
  () => phase.value === "checking" || phase.value === "downloading" || phase.value === "installing"
);
const hasUpdate = computed(() => update.value !== null || store.pendingUpdate !== null);
const updateTip = computed(() => {
  if (phase.value === "downloading") {
    return `正在下载 v${update.value?.version ?? ""}：${progress.value}%`;
  }
  if (phase.value === "ready") {
    return `v${store.pendingUpdate?.version ?? update.value?.version ?? ""} 已下载，点击「点击安装」完成更新`;
  }
  if (phase.value === "error") {
    return downloadError.value || "下载失败，点击重试";
  }
  if (update.value) return `发现新版本 v${update.value.version}，正在准备下载`;
  return `检查更新（当前 v${appVersion.value}）`;
});

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

onMounted(async () => {
  try {
    appVersion.value = await getVersion();
  } catch {
    /* 非 Tauri 环境下保持默认值 */
  }
  // 等 store 先把上次下载的更新读出来，再决定「直接显示安装」还是检查更新
  stopLoadedWatch = watch(
    () => store.loaded,
    (ready) => {
      if (ready) void initUpdate();
    },
    { immediate: true }
  );
});

onUnmounted(() => {
  if (startupTimer) clearTimeout(startupTimer);
  stopLoadedWatch?.();
  progressUnlisten?.();
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
    assets?: Array<{ name?: string; browser_download_url?: string; size?: number }>;
  };
  const version = String(rel.tag_name ?? "").replace(/^v/i, "");
  if (!version || !rel.html_url) throw new Error("GitHub 返回的 Release 数据不完整");
  const assets: ReleaseAsset[] = (rel.assets ?? []).flatMap((a) =>
    a.name && a.browser_download_url
      ? [{ name: a.name, url: a.browser_download_url, size: Number(a.size) || 0 }]
      : []
  );
  return {
    version,
    url: rel.html_url,
    name: rel.name ?? "",
    publishedAt: rel.published_at ?? "",
    assets,
  };
}

function rawError(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function checkError(e: unknown): string {
  const msg = rawError(e);
  if (/404/.test(msg)) return "GitHub 上还没有发布过版本";
  if (/403|rate limit/i.test(msg)) return "GitHub API 请求过于频繁，请稍后再试";
  return `检查更新失败：${msg}`;
}

function downloadErrorText(e: unknown): string {
  return `下载失败：${rawError(e)}`;
}

async function openRelease(url: string) {
  try {
    if (inTauri()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  } catch (e) {
    ElMessage.error(`打开浏览器失败：${rawError(e)}`);
  }
}

function openRepo() {
  void openRelease(REPO_URL);
}

/** 按当前平台挑选安装包：Windows 优先 NSIS，其次 MSI；macOS 用 DMG；Linux 优先 AppImage */
function assetFor(info: ReleaseInfo): ReleaseAsset | null {
  const os = store.platformInfo.os;
  const tests: RegExp[] =
    os === "windows"
      ? [/setup\.exe$/i, /\.msi$/i]
      : os === "macos"
        ? [/\.dmg$/i]
        : [/\.AppImage$/i, /\.deb$/i];
  for (const re of tests) {
    const found = info.assets.find((a) => re.test(a.name));
    if (found) return found;
  }
  return null;
}

/** 设置里的镜像地址 + 原始下载地址 */
function downloadUrl(raw: string): string {
  const mirror = store.network.mirror.trim();
  if (!mirror) return raw;
  return `${mirror.replace(/\/+$/, "")}/${raw}`;
}

function proxyValue(): string | null {
  return store.network.useProxy ? store.network.proxyUrl : null;
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

/** 启动时：先看上次是否已经下载完安装包；有就恢复「点击安装」，不再请求 GitHub */
async function initUpdate() {
  if (initialized) return;
  initialized = true;

  if (inTauri()) {
    try {
      progressUnlisten = await listen<{ downloaded: number; total: number }>(
        "update-progress",
        (event) => {
          const total = event.payload.total;
          progress.value =
            total > 0
              ? Math.min(100, Math.round((event.payload.downloaded / total) * 100))
              : Math.min(99, progress.value);
        }
      );
    } catch {
      /* 事件监听失败不影响下载本身 */
    }
  }

  const pending: PendingUpdate | null = store.pendingUpdate;
  if (pending) {
    if (!isNewer(pending.version, appVersion.value)) {
      store.clearPendingUpdate();
    } else {
      const size = await invoke<number>("update_file_size", { path: pending.path }).catch(() => 0);
      if (size > 0 && (!pending.size || size === pending.size)) {
        update.value = {
          version: pending.version,
          url: `${REPO_URL}/releases/tag/v${pending.version}`,
          name: "",
          publishedAt: "",
          assets: [],
        };
        progress.value = 100;
        phase.value = "ready";
        return;
      }
      store.clearPendingUpdate();
    }
  }

  startupTimer = window.setTimeout(() => void checkUpdate(false), 1500);
}

async function startDownload(info: ReleaseInfo, silent: boolean) {
  const asset = assetFor(info);
  if (!asset) {
    downloadError.value = "没有找到适用于当前系统的安装包";
    phase.value = "error";
    if (!silent) ElMessage.warning(`${downloadError.value}，可点击版本号打开发布页手动下载`);
    return;
  }

  phase.value = "downloading";
  progress.value = 0;
  downloadError.value = "";
  try {
    const path = await invoke<string>("update_download", {
      url: downloadUrl(asset.url),
      filename: asset.name,
      version: info.version,
      expectedSize: asset.size || null,
      proxy: proxyValue(),
    });
    store.setPendingUpdate({
      version: info.version,
      path,
      assetName: asset.name,
      size: asset.size,
    });
    progress.value = 100;
    phase.value = "ready";
    ElMessage.success(`v${info.version} 已下载，点击「点击安装」完成更新`);
  } catch (e) {
    downloadError.value = downloadErrorText(e);
    phase.value = "error";
    if (!silent) ElMessage.error(`${downloadError.value}，可点击版本号重试`);
  }
}

async function checkUpdate(manual: boolean) {
  if (store.pendingUpdate || phase.value === "ready") {
    if (manual) ElMessage.info("安装包已经下载好了，点击「点击安装」即可");
    return;
  }
  if (busy.value) return;
  phase.value = "checking";
  try {
    // Promise.all 同时等「请求」和「最短转圈」：
    // 请求秒回 / 秒失败时也要等满 650ms；请求慢则照常等它返回
    const [outcome] = await Promise.all([
      fetchLatest().then(
        (value) => ({ ok: true as const, value }),
        (error: unknown) => ({ ok: false as const, error })
      ),
      wait(MIN_CHECK_SPIN_MS),
    ]);
    if (!outcome.ok) throw outcome.error;

    const latest = outcome.value;
    if (isNewer(latest.version, appVersion.value)) {
      update.value = latest;
      if (inTauri()) {
        await startDownload(latest, !manual);
      } else {
        phase.value = "idle";
        if (manual) ElMessage.info(`发现新版本 v${latest.version}，请到发布页下载`);
      }
    } else {
      update.value = null;
      phase.value = "idle";
      if (manual) ElMessage.success(`已是最新版本 v${appVersion.value}`);
    }
  } catch (e) {
    phase.value = "idle";
    if (manual) ElMessage.warning(checkError(e));
  } finally {
    if (phase.value === "checking") phase.value = "idle";
  }
}

async function installUpdate() {
  const pending = store.pendingUpdate;
  if (!pending) return;
  try {
    await ElMessageBox.confirm(
      `安装包 v${pending.version} 已下载完成。点击「安装」会启动安装程序，当前应用将退出。`,
      "安装更新",
      { confirmButtonText: "安装", cancelButtonText: "稍后", type: "info" }
    );
  } catch {
    return;
  }
  phase.value = "installing";
  try {
    const kind = await invoke<string>("update_install", { path: pending.path });
    if (kind === "opened") {
      phase.value = "ready";
      ElMessage.info("已打开安装包，请按系统提示完成安装");
    } else {
      ElMessage.success("正在启动安装程序，应用即将退出…");
    }
  } catch (e) {
    phase.value = "ready";
    ElMessage.error(`启动安装程序失败：${rawError(e)}`);
  }
}

function retryDownload() {
  if (!update.value) {
    void checkUpdate(true);
    return;
  }
  if (downloadError.value.includes("没有找到适用于当前系统")) {
    void openRelease(update.value.url);
    return;
  }
  void startDownload(update.value, false);
}

function onUpdateClick() {
  if (phase.value === "ready") {
    void installUpdate();
    return;
  }
  if (phase.value === "error") {
    retryDownload();
    return;
  }
  if (phase.value === "downloading" || phase.value === "checking" || phase.value === "installing") {
    return;
  }
  void checkUpdate(true);
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
      <span class="mono">{{ store.defaultVersion || (store.hasInstalledFrp ? "未设置" : "未安装") }}</span>
    </span>
    <span class="sep" />
    <span class="item">
      <span class="label">配置</span>
      <span class="mono">config/store.json</span>
    </span>

    <span class="flex" />

    <span class="item muted">
      <span class="mono">最后刷新 {{ stamp() }}</span>
    </span>
    <span class="sep" />
    <span class="item muted version-item">
      <!-- 检测更新：点一下查 GitHub latest release；检测到新版后自动下载 -->
      <button type="button" class="update" :class="{ busy: phase === 'checking', fresh: hasUpdate }" :title="updateTip"
        :aria-label="updateTip" :disabled="busy" @click="onUpdateClick">
        <AppIcon name="refresh" :size="11" />
        <span v-if="hasUpdate" class="dot" />
      </button>
      <span class="label">版本</span>
      <button v-if="phase === 'ready'" type="button" class="version-action"
        :title="`安装 v${store.pendingUpdate?.version ?? ''}`" @click="installUpdate">
        点击安装
      </button>
      <button v-else-if="phase === 'downloading'" type="button" class="version-action downloading" :title="updateTip"
        disabled>
        下载 {{ progress }}%
      </button>
      <button v-else-if="phase === 'error'" type="button" class="version-action error" :title="downloadError"
        @click="retryDownload">
        重试下载
      </button>
      <span v-else class="mono">v{{ appVersion }}</span>
      <span v-if="phase === 'downloading'" class="version-progress">
        <span :style="{ width: `${progress}%` }" />
      </span>
    </span>
    <span class="sep" />
    <span class="item muted version-item">
      <button type="button" class="github" title="打开 GitHub 仓库" aria-label="打开 GitHub 仓库" @click="openRepo">
        <Github :size="11" :stroke-width="1.7" />
      </button>
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

/* 检测更新 / GitHub 图标：18×18 圆形热区 */
.update,
.github {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 50%;
  background: transparent;
  color: inherit;
  cursor: pointer;
  transition: color 0.1s ease, border-color 0.1s ease, background-color 0.1s ease;
}

.update:hover:not(:disabled),
.github:hover {
  color: var(--text);
  border-color: var(--line-strong);
  background: var(--bg-hover);
}

.update:focus-visible,
.github:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
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

.github :deep(svg) {
  display: block;
}

.version-item {
  position: relative;
}

.version-action {
  padding: 0 2px;
  border: 0;
  background: transparent;
  color: var(--accent);
  font: inherit;
  font-size: var(--fs-1);
  cursor: pointer;
  white-space: nowrap;
}

.version-action:hover {
  text-decoration: underline;
}

.version-action.downloading {
  color: var(--amber);
  cursor: progress;
}

.version-action.error {
  color: var(--red);
}

.version-progress {
  position: absolute;
  left: 0;
  right: 0;
  bottom: -2px;
  height: 1px;
  background: var(--line);
  overflow: hidden;
}

.version-progress>span {
  display: block;
  height: 100%;
  background: var(--accent);
  transition: width 0.1s linear;
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
