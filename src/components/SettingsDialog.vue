<script setup lang="ts">
import { nextTick, reactive, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore, type ExternalFrp, type ServerConfig, type SettingsFocus } from "@/stores/app";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ modelValue: boolean; focus?: SettingsFocus }>();
const emit = defineEmits<{ (e: "update:modelValue", v: boolean): void }>();

const store = useAppStore();
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const frpSection = ref<HTMLElement | null>(null);
const serverSection = ref<HTMLElement | null>(null);
const flash = ref<SettingsFocus>(null);
let flashTimer: ReturnType<typeof setTimeout> | undefined;

const draft = reactive<ServerConfig>({ ...store.server });
const dirPath = ref(store.paths.projects);
const cfgPath = ref(store.paths.config);
const netDraft = reactive({ ...store.network });
const mirrorDraft = ref(store.network.mirror);
watch(mirrorDraft, (v) => {
  store.updateNetwork({ ...store.network, mirror: v });
});
const picked = ref(false);
const pickingFrpc = ref(false);
const backupBusy = ref(false);

watch(
  () => props.modelValue,
  (open) => {
    if (!open) {
      flash.value = null;
      return;
    }
    Object.assign(draft, store.server);
    dirPath.value = store.paths.projects;
    cfgPath.value = store.paths.config;
    Object.assign(netDraft, store.network);
    mirrorDraft.value = store.network.mirror;
    void store.fetchReleases();
    // 等弹窗过渡结束再定位，否则 scrollIntoView 会被动画打断
    setTimeout(() => focusSection(props.focus ?? null), 120);
  }
);

/** 打开设置页时定位到指定分区，并短暂高亮，避免用户找不到 */
function focusSection(focus: SettingsFocus) {
  if (!focus) return;
  const el = focus === "frp" ? frpSection.value : serverSection.value;
  if (!el) return;
  void nextTick(() => {
    el.scrollIntoView({ block: "start", behavior: "smooth" });
    flash.value = focus;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      if (flash.value === focus) flash.value = null;
    }, 1800);
  });
}

watch(
  () => props.focus,
  (f) => {
    if (props.modelValue && f) focusSection(f);
  }
);

// 服务器设置改动即时生效，不再需要「应用」按钮
watch(draft, () => store.updateServer({ ...draft }), { deep: true });
watch(netDraft, () => store.updateNetwork({ ...netDraft }), { deep: true });

function close() {
  emit("update:modelValue", false);
}

function progressText(v: string) {
  const p = store.installProgress[v];
  if (!p) return "安装";
  const map: Record<string, string> = {
    connect: "连接中…",
    download: `${p.pct}%`,
    verify: "校验中…",
    extract: "解压中…",
    done: "完成",
  };
  return map[p.phase] ?? "安装";
}

async function install(v: string) {
  const r = await store.installVersion(v);
  if (r.ok) ElMessage({ message: `已安装 frp ${v}`, type: "success", duration: 1600 });
  else ElMessage({ message: `安装失败：${r.reason}`, type: "error", duration: 8000, showClose: true });
}

async function remove(v: string) {
  const r = await store.removeVersion(v);
  ElMessage({
    message: r.ok ? `已删除 frp ${v}` : r.reason ?? "删除失败",
    type: r.ok ? "success" : "warning",
    duration: r.ok ? 1600 : 2600,
  });
}

function setDefault(v: string) {
  store.setDefaultVersion(v);
  ElMessage({ message: `已将 ${v} 设为默认版本`, type: "success", duration: 1600 });
}

function savePaths() {
  store.updatePaths({ projects: dirPath.value, config: cfgPath.value });
  picked.value = true;
}

async function pickPath(kind: "dir" | "file") {
  if (!("__TAURI_INTERNALS__" in window)) {
    ElMessage({
      message: "浏览器预览没有系统选择框，直接编辑路径即可",
      type: "warning",
      duration: 2200,
    });
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    directory: kind === "dir",
    multiple: false,
    title: kind === "dir" ? "选择项目目录" : "选择配置文件",
  });
  if (typeof selected === "string" && selected) {
    if (kind === "dir") dirPath.value = selected;
    else cfgPath.value = selected;
    savePaths();
  }
}

function alreadyInRepo(item: ExternalFrp) {
  return store.versionList.some((v) => v.version === item.version && v.installed);
}

async function takeOver(item: ExternalFrp) {
  try {
    await ElMessageBox.confirm(
      `将 ${item.path} 复制到程序目录 versions/${item.version}/，之后就能和内置版本一样启动项目、安装服务。原文件不会被修改。`,
      "接管外部 frp",
      { confirmButtonText: "接管", cancelButtonText: "取消" },
    );
  } catch {
    return;
  }
  const r = await store.takeOverExternal(item);
  if (r.ok) {
    ElMessage({ message: `已接管 frp ${item.version}`, type: "success", duration: 1800 });
  } else {
    ElMessage({ message: `接管失败：${r.reason}`, type: "error", duration: 8000, showClose: true });
  }
}

function reopenGuide() {
  close();
  store.openGuide();
}

/** 自动扫描找不到时，手动指定一个 frpc[.exe]：先只读探测版本，确认后再复制进版本库 */
async function pickFrpcFile() {
  if (pickingFrpc.value) return;
  if (!inTauri) {
    ElMessage({
      message: "浏览器预览无法读取本机文件，请在桌面应用中选择",
      type: "warning",
      duration: 2400,
    });
    return;
  }
  pickingFrpc.value = true;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: false,
      multiple: false,
      title: "选择 frpc 可执行文件",
      filters:
        store.platformInfo.os === "windows"
          ? [{ name: "frpc 可执行文件", extensions: ["exe"] }]
          : undefined,
    });
    if (typeof selected !== "string" || !selected) return;

    const r = await store.probeExternalFile(selected);
    if (!r.ok || !r.item) {
      ElMessage({
        message: `读取失败：${r.reason ?? "未知错误"}`,
        type: "error",
        duration: 8000,
        showClose: true,
      });
      return;
    }
    if (alreadyInRepo(r.item)) {
      ElMessage({
        message: `frp ${r.item.version} 已经在版本库里了，可在上面的列表里设为默认`,
        type: "info",
        duration: 3000,
      });
      return;
    }
    await takeOver(r.item);
  } catch (e) {
    ElMessage({
      message: `选择文件失败：${String(e)}`,
      type: "error",
      duration: 6000,
      showClose: true,
    });
  } finally {
    pickingFrpc.value = false;
  }
}

/** 导出文件名带日期，避免多次导出互相覆盖 */
function configFileName() {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `frp-tunnel-manager-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(
    d.getHours(),
  )}${pad(d.getMinutes())}.json`;
}

function downloadInBrowser(content: string, filename: string) {
  const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** 导出项目、服务器、网络、目录与默认版本；不含运行状态 */
async function exportConfig() {
  if (backupBusy.value) return;
  backupBusy.value = true;
  try {
    const content = store.exportConfigJson();
    const filename = configFileName();
    if (!inTauri) {
      downloadInBrowser(content, filename);
      ElMessage({ message: "配置已导出", type: "success", duration: 1800 });
      return;
    }
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: filename,
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (typeof path !== "string" || !path) return;
    await invoke("config_write_file", { path, content });
    ElMessage({ message: "配置已导出", type: "success", duration: 1800 });
  } catch (e) {
    ElMessage({
      message: `导出失败：${String(e)}`,
      type: "error",
      duration: 6000,
      showClose: true,
    });
  } finally {
    backupBusy.value = false;
  }
}

/** 导入配置：只覆盖应用配置，不删除磁盘上已有的项目目录 */
async function importConfig() {
  if (backupBusy.value) return;
  if (!inTauri) {
    ElMessage({
      message: "浏览器预览无法读取本机配置，请在桌面应用中使用导入",
      type: "warning",
      duration: 3000,
    });
    return;
  }
  backupBusy.value = true;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: false,
      multiple: false,
      title: "选择配置文件",
      filters: [{ name: "JSON 配置", extensions: ["json"] }],
    });
    if (typeof selected !== "string" || !selected) return;

    const text = await invoke<string>("config_read_file", { path: selected });
    const parsed = store.parseConfigText(text);
    if (!parsed.ok || !parsed.data) {
      ElMessage({
        message: `配置无效：${parsed.reason ?? "未知错误"}`,
        type: "error",
        duration: 8000,
        showClose: true,
      });
      return;
    }

    if (store.projects.some((p) => p.serviceInstalled)) {
      ElMessage({
        message: "当前有已安装为服务的项目，请先卸载服务再导入配置",
        type: "warning",
        duration: 5000,
      });
      return;
    }

    const running = store.projects.filter((p) => p.state === "running" || p.state === "starting");
    const names = parsed.names?.length ? `包含：${parsed.names.join("、")}` : "";
    await ElMessageBox.confirm(
      `将导入 ${parsed.count ?? 0} 个项目${names ? `（${names}）` : ""}，并覆盖当前的服务器、传输、网络、目录与默认版本配置。${
        running.length ? `当前有 ${running.length} 个项目正在运行，导入前会先停止它们。` : ""
      }此操作不可撤销，但不会删除磁盘上已有的项目目录。`,
      "导入配置",
      { confirmButtonText: "导入", cancelButtonText: "取消", type: "warning" },
    );

    for (const p of running) await store.stopProject(p.id);
    const applied = await store.applyConfigData(parsed.data);
    if (!applied.ok) {
      ElMessage({
        message: applied.reason ?? "导入失败",
        type: "error",
        duration: 6000,
        showClose: true,
      });
      return;
    }
    close();
    ElMessage({ message: `已导入 ${applied.count ?? 0} 个项目`, type: "success", duration: 2200 });
  } catch (e) {
    if (e === "cancel" || e === "close") return;
    ElMessage({
      message: `导入失败：${String(e)}`,
      type: "error",
      duration: 6000,
      showClose: true,
    });
  } finally {
    backupBusy.value = false;
  }
}
</script>

<template>
  <el-dialog
    :model-value="props.modelValue"
    width="720px"
    :show-close="false"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    align-center
    class="settings-dialog"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <template #header>
      <div class="dlg-head">
        <div class="head-text">
          <div class="dlg-title">设置</div>
          <div class="dlg-sub">FRP 版本、全局服务器与数据目录</div>
        </div>
        <button class="btn icon" @click="close"><AppIcon name="close" :size="13" /></button>
      </div>
    </template>

    <el-scrollbar max-height="62vh" class="settings-scroll">
      <div class="settings">
        <!-- FRP 版本 -->
        <div ref="frpSection" class="section" :class="{ 'focus-flash': flash === 'frp' }">
          <div class="section-head">
            <span class="head-left">
              <span class="label">FRP 版本</span>
              <span v-if="store.defaultVersion" class="tag on mono">默认 {{ store.defaultVersion }}</span>
              <span v-else-if="store.hasInstalledFrp" class="tag mono">未设置默认</span>
              <span v-else class="tag err mono">未安装</span>
            </span>
            <span class="hint mono">
              {{
                store.releaseLoading
                  ? "正在获取 GitHub Releases…"
                  : store.releaseError || "来源 GitHub Releases"
              }}
            </span>
          </div>

          <div v-if="!store.hasInstalledFrp" class="notice">
            <AppIcon name="warn" :size="13" />
            <span>检测到 FRP 未安装，请先安装一个。选一个版本点右侧「安装」；如果本机已有 frpc，也可以在下方「外部安装」里扫描或手动选择。</span>
          </div>

          <el-scrollbar max-height="228px" class="repo-scroll">
          <div class="repo">
            <div v-for="v in store.versionList" :key="v.version" class="repo-row">
              <span class="mono ver">{{ v.version }}</span>
              <span class="tag" :class="{ on: v.installed }">{{ v.installed ? "已安装" : "未安装" }}</span>
              <span class="hint mono">{{ store.versionUsage(v.version) }} 个项目</span>
              <span class="hint mono size">{{ v.installed ? v.size ?? "—" : "—" }}</span>
              <span class="spacer" />
              <button v-if="v.installed && v.version === store.defaultVersion" class="btn current" disabled>
                已默认
              </button>
              <button v-else-if="v.installed" class="btn" @click="setDefault(v.version)">
                设为默认
              </button>
              <button v-if="v.installed" class="btn danger" @click="remove(v.version)">
                <AppIcon name="trash" :size="12" />删除
              </button>
              <button
                v-else
                class="btn accent"
                :disabled="!!store.installProgress[v.version] && store.installProgress[v.version].phase !== 'done'"
                @click="install(v.version)"
              >
                <AppIcon name="down" :size="12" />{{ progressText(v.version) }}
              </button>
            </div>
            <div v-if="store.versionList.length === 0" class="repo-empty hint">没有可用的版本</div>
          </div>
          </el-scrollbar>

          <div class="sub-head">
            <span class="label">外部安装</span>
            <span class="hint">扫描或手动指定本机已有的 frpc</span>
            <span class="spacer" />
            <button class="btn sm" :disabled="pickingFrpc" @click="pickFrpcFile">
              <AppIcon name="file" :size="11" />{{ pickingFrpc ? "读取中…" : "选择 frpc…" }}
            </button>
            <button class="btn sm" :disabled="store.externalScanning" @click="store.scanExternal()">
              <AppIcon name="refresh" :size="11" :class="{ spin: store.externalScanning }" />
              重新扫描
            </button>
          </div>
          <div v-if="!inTauri" class="ext-note hint">
            浏览器预览无法扫描本机；桌面版会检查 PATH 与常见安装目录。
          </div>
          <div v-else-if="store.externalScanning && !store.externalScanned" class="ext-note hint">
            正在扫描 PATH 与常见安装目录…
          </div>
          <div v-else-if="store.externalVersions.length === 0" class="ext-note hint">
            {{ store.externalScanned ? "没有检测到外部安装的 frpc。" : "尚未扫描。" }}
          </div>
          <div v-for="e in store.externalVersions" :key="e.path" class="ext-row">
            <span class="tag on mono">外部</span>
            <span class="mono ver">{{ e.version }}</span>
            <span class="mono ext-path" :title="e.path">{{ e.path }}</span>
            <span class="tag">{{ e.source }}</span>
            <span class="spacer" />
            <span v-if="alreadyInRepo(e)" class="hint">已在版本库</span>
            <button v-else class="btn sm" @click="takeOver(e)">接管</button>
          </div>
        </div>

        <!-- 全局服务器 -->
        <div ref="serverSection" class="section" :class="{ 'focus-flash': flash === 'server' }">
          <div class="section-head"><span class="label">全局服务器</span></div>
          <div class="rows">
            <div class="row">
              <label>服务器地址</label>
              <div class="ctl">
                <el-input v-model="draft.addr" placeholder="例如 203.0.113.10" />
              </div>
            </div>
            <div class="row">
              <label>服务器端口</label>
              <div class="ctl">
                <el-input-number v-model="draft.port" :min="1" :max="65535" controls-position="right" />
              </div>
            </div>
            <div class="row">
              <label>传输协议</label>
              <div class="ctl">
                <el-select v-model="draft.protocol">
                  <el-option label="KCP" value="kcp" />
                  <el-option label="TCP" value="tcp" />
                  <el-option label="QUIC" value="quic" />
                  <el-option label="WebSocket" value="ws" />
                </el-select>
              </div>
            </div>
            <div class="row">
              <label>TLS</label>
              <div class="ctl switch-ctl">
                <el-switch v-model="draft.tls" />
                <span class="hint">开启后 frpc 与 frps 之间使用 TLS 加密</span>
              </div>
            </div>
            <div class="row">
              <label>认证方式</label>
              <div class="ctl">
                <el-select v-model="draft.authMethod">
                  <el-option label="Token" value="token" />
                  <el-option label="不认证" value="none" />
                </el-select>
              </div>
            </div>
            <div class="row">
              <label>Token</label>
              <div class="ctl">
                <el-input
                  v-model="draft.token"
                  type="password"
                  show-password
                  :disabled="draft.authMethod === 'none'"
                  placeholder="与 frps.toml 里的 auth.token 保持一致"
                />
                <span class="hint">属于客户端级配置，所有项目共用；改动后重启对应服务生效</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 网络代理 -->
        <!-- 传输优化 -->
        <div class="section">
          <div class="section-head"><span class="label">传输优化</span></div>
          <div class="rows">
            <div class="row">
              <label>TCP 多路复用</label>
              <div class="ctl switch-ctl">
                <el-switch v-model="store.globalTransport.tcpMux" />
                <span class="hint">多条代理复用一条连接，减少握手与重连开销</span>
              </div>
            </div>
            <div class="row">
              <label>连接池大小</label>
              <div class="ctl">
                <el-input-number v-model="store.globalTransport.poolCount" :min="0" :max="50" controls-position="right" />
                <span class="hint">0 表示不启用连接池；项目可在配置页单独覆盖</span>
              </div>
            </div>
            <div class="row">
              <label>心跳间隔（秒）</label>
              <div class="ctl">
                <el-input-number v-model="store.globalTransport.heartbeatInterval" :min="10" :max="600" controls-position="right" />
                <span class="hint">与 frps 保持连接的心跳间隔，默认 30 秒</span>
              </div>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="section-head">
            <span class="label">网络代理</span>
            <span class="hint mono">访问 GitHub 用</span>
          </div>
          <div class="rows">
            <div class="row">
              <label>启用代理</label>
              <div class="ctl">
                <el-switch v-model="netDraft.useProxy" />
                <span class="hint">拉取版本列表、下载安装包时走代理；GitHub 直连超时时使用。</span>
              </div>
            </div>
            <div v-if="netDraft.useProxy" class="row">
              <label>代理地址</label>
              <div class="ctl">
                <el-input v-model="netDraft.proxyUrl" placeholder="http://127.0.0.1:7890" />
                <span class="hint">支持 http:// 与 socks5:// 前缀，例如本地 Clash 的 7890 端口。</span>
              </div>
            </div>
            <div class="row">
              <label>下载镜像</label>
              <div class="ctl">
                <el-input v-model="mirrorDraft" placeholder="https://gh-proxy.com/" />
                <span class="hint">
                  GitHub 直连超时时，用它给 release 下载地址加速；留空表示直连。
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- 数据目录 -->
        <div class="section">
          <div class="section-head"><span class="label">数据目录</span></div>
          <div class="rows">
            <div class="row">
              <label>项目目录</label>
              <div class="ctl path-ctl">
                <el-input v-model="dirPath" @change="savePaths" />
                <button class="btn" @click="pickPath('dir')">
                  <AppIcon name="folder" :size="12" />选择
                </button>
              </div>
            </div>
            <div class="row">
              <label>应用配置</label>
              <div class="ctl path-ctl">
                <el-input v-model="cfgPath" @change="savePaths" />
                <button class="btn" @click="pickPath('file')">
                  <AppIcon name="folder" :size="12" />选择
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 配置备份 -->
        <div class="section">
          <div class="section-head">
            <span class="label">配置备份</span>
            <span class="tag">JSON</span>
          </div>
          <div class="rows">
            <div class="row">
              <label>导入 / 导出</label>
              <div class="ctl backup-ctl">
                <button class="btn" :disabled="backupBusy" @click="exportConfig">
                  <AppIcon name="down" :size="12" />导出配置
                </button>
                <button class="btn" :disabled="backupBusy" @click="importConfig">
                  <AppIcon name="up" :size="12" />导入配置
                </button>
                <span class="hint">
                  把项目、服务器、网络与目录设置保存成一个 JSON 文件，换机或重装后可以恢复。文件内含服务器 token，请妥善保管。
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- 帮助 -->
        <div class="section">
          <div class="section-head"><span class="label">帮助</span></div>
          <div class="rows">
            <div class="row">
              <label>新手引导</label>
              <div class="ctl">
                <button class="btn" @click="reopenGuide">
                  <AppIcon name="bolt" :size="12" />
                  重新查看引导
                </button>
                <span class="hint">回顾「配置服务器 → 安装 frpc → 新建项目」三步。</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </el-scrollbar>
  </el-dialog>
</template>

<style scoped>
.dlg-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--sp-2);
}
.dlg-title {
  font-size: var(--fs-4);
  font-weight: 600;
}
.dlg-sub {
  font-size: var(--fs-2);
  color: var(--text-faint);
}
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
/* 滚动条落在弹窗内边距里，而不是压在内容上。
   el-dialog 的 body 在 teleport 出来的节点里，scoped 的 :deep 匹配不到，必须用 :global 限定在 .settings-dialog 下 */
:global(.settings-dialog .el-dialog__body) {
  padding: 0;
}
.settings-scroll {
  box-sizing: border-box;
  padding: var(--sp-3);
}
.repo {
  display: flex;
  flex-direction: column;
}
.repo-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-1) var(--sp-2);
  box-shadow: 0 1px 0px var(--line);
  height: 38px;
}
.repo-row:last-child {
  border-bottom: none;
}
.repo-row:hover {
  background: var(--bg-hover);
}
.ver {
  width: 72px;
  flex: none;
  color: var(--text);
}
.size {
  width: 64px;
  flex: none;
  text-align: right;
}
.spacer {
  flex: 1;
}
.repo-empty {
  padding: var(--sp-2);
}

.notice {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin: var(--sp-2) var(--sp-2);
  padding: 8px 10px;
  font-size: var(--fs-2);
  line-height: 1.55;
  color: color-mix(in srgb, var(--amber) 85%, var(--text));
  border: 1px solid color-mix(in srgb, var(--amber) 35%, var(--line));
  background: color-mix(in srgb, var(--amber) 8%, transparent);
  border-radius: var(--radius);
}
.notice :deep(.app-icon) {
  margin-top: 1px;
}
.sub-head {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  margin: var(--sp-2) var(--sp-2) 0;
  padding-top: var(--sp-2);
  border-top: 1px dashed var(--line);
}
.sub-head .spacer {
  flex: 1;
}
.ext-note {
  padding: var(--sp-1) var(--sp-2) var(--sp-2);
}
.ext-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: 38px;
  padding: var(--sp-1) var(--sp-2);
  box-shadow: 0 1px 0 var(--line);
  font-size: var(--fs-2);
}
.ext-row:hover {
  background: var(--bg-hover);
}
.ext-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-faint);
}
.spin {
  animation: spin 1s linear infinite;
}
.section {
  border-radius: var(--radius);
}
.focus-flash {
  animation: focus-flash 1.7s ease-out;
}
@keyframes focus-flash {
  0%,
  55% {
    box-shadow: 0 0 0 1px var(--accent-line), 0 0 0 4px var(--accent-soft);
  }
  100% {
    box-shadow: 0 0 0 1px transparent, 0 0 0 4px transparent;
  }
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .spin,
  .focus-flash {
    animation: none;
  }
}
.ctl :deep(.el-input),
.ctl :deep(.el-select),
.ctl :deep(.el-input-number) {
  width: 100%;
}
.switch-ctl {
  flex-direction: row;
  align-items: center;
  gap: var(--sp-2);
}
.backup-ctl {
  flex-direction: row;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-1);
}
.backup-ctl .hint {
  flex-basis: 100%;
}
.path-ctl {
  flex-direction: row;
  align-items: center;
  gap: var(--sp-1);
}
.path-ctl .btn {
  flex: none;
  height: 30px;
}
.path-ctl :deep(.el-input) {
  height: 30px;
}
.path-ctl :deep(.el-input__wrapper) {
  height: 30px;
}
.head-left {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}
.repo-row .btn.current:disabled {
  opacity: 1;
  color: var(--accent);
  border-color: var(--accent-line);
  background: var(--accent-soft);
  cursor: default;
}
</style>
