<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { ElMessage } from "element-plus";
import { useAppStore, type ServerConfig } from "@/stores/app";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ (e: "update:modelValue", v: boolean): void }>();

const store = useAppStore();

const draft = reactive<ServerConfig>({ ...store.server });
const dirPath = ref(store.paths.projects);
const cfgPath = ref(store.paths.config);
const netDraft = reactive({ ...store.network });
const mirrorDraft = ref(store.network.mirror);
watch(mirrorDraft, (v) => {
  store.updateNetwork({ ...store.network, mirror: v });
});
const picked = ref(false);

watch(
  () => props.modelValue,
  (open) => {
    if (!open) return;
    Object.assign(draft, store.server);
    dirPath.value = store.paths.projects;
    cfgPath.value = store.paths.config;
    Object.assign(netDraft, store.network);
    mirrorDraft.value = store.network.mirror;
    void store.fetchReleases();
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
        <div class="section">
          <div class="section-head">
            <span class="head-left">
              <span class="label">FRP 版本</span>
              <span class="tag on mono">默认 {{ store.defaultVersion }}</span>
            </span>
            <span class="hint mono">
              {{
                store.releaseLoading
                  ? "正在获取 GitHub Releases…"
                  : store.releaseError || "来源 GitHub Releases"
              }}
            </span>
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
        </div>

        <!-- 全局服务器 -->
        <div class="section">
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
