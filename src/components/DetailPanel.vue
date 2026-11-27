<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import type { ScrollbarInstance } from "element-plus";
import { useAppStore } from "@/stores/app";
import type { IconName } from "@/icons";
import type { ServiceState } from "@/types";
import AppIcon from "./AppIcon.vue";
import OverviewTab from "./OverviewTab.vue";
import ConfigTab from "./ConfigTab.vue";
import LogsTab from "./LogsTab.vue";

const store = useAppStore();
const emit = defineEmits<{
  (e: "new-project"): void;
  (e: "open-guide"): void;
}>();
const p = computed(() => store.selected);
const busy = ref(false);
const ovScroll = ref<ScrollbarInstance>();
const cfgScroll = ref<ScrollbarInstance>();
const logScroll = ref<ScrollbarInstance>();

/** 切换项目时三个滚动区都回到顶部，避免继承上一个项目的位置 */
watch(
  () => store.selectedId,
  async () => {
    await nextTick();
    ovScroll.value?.setScrollTop(0);
    cfgScroll.value?.setScrollTop(0);
    logScroll.value?.setScrollTop(0);
  },
);

const stateText: Record<ServiceState, string> = {
  running: "运行中",
  stopped: "已停止",
  starting: "启动中",
  stopping: "停止中",
  error: "错误",
  "not-installed": "未安装",
};

const iconName = computed<IconName>(() => (p.value?.icon as IconName) ?? "file");
const installed = computed(() => !!p.value && p.value.serviceInstalled === true);
const running = computed(() => p.value?.state === "running" || p.value?.state === "starting");

async function run(fn: () => Promise<void>, ok: string) {
  if (busy.value) return;
  busy.value = true;
  try {
    await fn();
    ElMessage.success(ok);
  } catch (e) {
    const msg = String(e);
    if (store.platformInfo.needsAdmin && /管理员/.test(msg)) {
      try {
        await ElMessageBox.confirm(
          `安装 / 卸载${store.platformInfo.serviceKind}需要管理员权限。是否以管理员身份重新启动本程序？`,
          "需要管理员权限",
          { confirmButtonText: "以管理员身份重启", cancelButtonText: "取消", type: "warning" },
        );
        await store.relaunchElevated();
        return;
      } catch {
        /* 用户取消 */
      }
    }
    ElMessage.error({ message: msg, duration: 6000, showClose: true });
  } finally {
    busy.value = false;
  }
}

function toggle() {
  if (!p.value) return;
  if (running.value) {
    void run(() => store.stopProject(p.value!.id), "已停止");
    return;
  }
  const miss = missingFrpVersion();
  if (miss !== null) {
    void needFrp(miss);
    return;
  }
  void run(() => store.startProject(p.value!.id), "已启动，正在连接服务器…");
}

function restart() {
  if (!p.value) return;
  const miss = missingFrpVersion();
  if (miss !== null) {
    void needFrp(miss);
    return;
  }
  void run(() => store.restartProject(p.value!.id), "已重启");
}

function toggleInstall() {
  if (!p.value) return;
  if (installed.value) {
    void run(() => store.uninstallService(p.value!.id), "服务已卸载");
    return;
  }
  const miss = missingFrpVersion();
  if (miss !== null) {
    void needFrp(miss);
    return;
  }
  void run(() => store.installService(p.value!.id), `已安装为${store.platformInfo.serviceKind}`);
}

/** 项目当前版本是否可用；不可用时返回版本号（空串表示完全没装） */
function missingFrpVersion(): string | null {
  const proj = p.value;
  if (!proj) return null;
  if (!proj.version && store.defaultVersion) proj.version = store.defaultVersion;
  if (store.isVersionInstalled(proj.version)) return null;
  return proj.version || "";
}

async function removeProject() {
  const proj = p.value;
  if (!proj) return;
  if (proj.serviceInstalled) {
    ElMessage.warning("请先卸载服务，再删除项目");
    return;
  }
  const extra = running.value ? "项目正在运行，删除前会先停止。" : "";
  try {
    await ElMessageBox.confirm(
      `删除「${proj.name}」后，本地的 frpc.toml 与运行日志会一并删除，且无法恢复。${extra}`,
      "删除项目",
      {
        confirmButtonText: "删除",
        cancelButtonText: "取消",
        type: "warning",
        confirmButtonClass: "el-button--danger",
      },
    );
  } catch {
    return;
  }
  const id = proj.id;
  await run(() => store.deleteProject(id), "项目已删除");
}

/** 未安装 frpc 时的统一提示：确认后直接跳到设置页的 FRP 版本区 */
async function needFrp(version: string) {
  const text = version
    ? `检测到项目使用的 frp ${version} 尚未安装。请先在「设置 → FRP 版本」里安装一个，再回来启动项目或安装服务。`
    : "检测到 FRP 未安装，请先安装一个。安装完成后回到这里，就能启动项目或安装服务。";
  try {
    await ElMessageBox.alert(text, "需要先安装 FRP 客户端", {
      confirmButtonText: "去安装",
      type: "warning",
    });
  } catch {
    return;
  }
  store.openSettings("frp");
}
</script>

<template>
  <section v-if="p" class="detail">
    <!-- 项目头：唯一一处服务操作入口 -->
    <header class="dhead">
      <div class="title-row">
        <span class="p-ico"><AppIcon :name="iconName" :size="16" /></span>
        <h1>{{ p.proxy.name || p.name }}</h1>
        <span class="tag">{{ p.template }}</span>
        <span class="tag" :class="{ on: p.state === 'running', err: p.state === 'error' }">
          {{ stateText[p.state] }}
        </span>
        <span
          v-if="store.platformInfo.needsAdmin && !store.isAdmin"
          class="tag"
          title="安装 / 卸载服务需要管理员权限"
          >非管理员</span
        >
        <span v-if="installed" class="tag on" :title="`由${store.platformInfo.serviceKind}托管运行`">服务</span>
      </div>
      <div class="actions">
        <button class="btn" :disabled="busy || installed" @click="toggle">
          <AppIcon :name="running ? 'stop' : 'play'" :size="13" />
          {{ running ? "停止" : "启动" }}
        </button>
        <button class="btn" :disabled="busy || installed" @click="restart">
          <AppIcon name="restart" :size="13" />
          重启
        </button>
        <button class="btn" :class="installed ? 'danger' : 'accent'" :disabled="busy" @click="toggleInstall">
          <AppIcon :name="installed ? 'uninstall' : 'install'" :size="13" />
          {{ installed ? "卸载服务" : "安装服务" }}
        </button>
        <button class="btn danger" :disabled="busy" title="删除项目及本地配置与日志" @click="removeProject">
          <AppIcon name="trash" :size="13" />
          删除
        </button>
      </div>
    </header>

    <!-- 上：左概述 / 右配置 -->
    <div class="panels">
      <section class="pane">
        <div class="pane-head">
          <span class="label">概述</span>
          <span class="hint mono">{{ p.proxy.name }}</span>
          <span id="ov-actions" class="pane-actions"></span>
        </div>
        <div class="pane-body"><el-scrollbar ref="ovScroll"><OverviewTab /></el-scrollbar></div>
      </section>

      <section class="pane pane-config">
        <div class="pane-head">
          <span class="label">配置</span>
          <span class="hint mono">frpc.toml</span>
          <span id="cfg-actions" class="pane-actions"></span>
        </div>
        <div class="pane-body"><el-scrollbar ref="cfgScroll"><ConfigTab /></el-scrollbar></div>
      </section>
    </div>

    <!-- 下：日志占满整行 -->
    <section class="pane logs-pane">
      <div class="pane-head">
        <span class="label">运行日志</span>
        <span class="hint mono">./projects/{{ p.id }}/frpc.log</span>
        <span class="pane-actions">
          <button class="btn sm">
            <AppIcon name="trash" :size="12" />
            清空
          </button>
          <button class="btn sm" :class="{ accent: store.logAutoScroll }" @click="store.toggleLogAutoScroll()">
            <AppIcon name="down" :size="12" />
            自动滚动
          </button>
        </span>
      </div>
      <div class="pane-body"><el-scrollbar ref="logScroll"><LogsTab /></el-scrollbar></div>
    </section>
  </section>

  <!-- 没有项目 / 没有选中项目时的空状态 -->
  <section v-else class="detail empty">
    <div class="empty-wrap">
      <div class="empty-ico"><AppIcon :name="store.projects.length ? 'inbox' : 'plus'" :size="24" /></div>
      <h2>{{ store.projects.length ? "选择一个项目" : "还没有项目" }}</h2>
      <p v-if="store.projects.length">从左侧列表选择要管理的隧道，或者新建一个项目。</p>
      <p v-else>
        先安装 frpc、配置服务器，再新建第一条隧道。frpc、配置与日志都放在程序目录，便携、不写系统目录。
      </p>
      <div class="empty-actions">
        <button class="btn accent" @click="emit('new-project')">
          <AppIcon name="plus" :size="12" />
          新建项目
        </button>
        <button class="btn" @click="emit('open-guide')">
          <AppIcon name="bolt" :size="12" />
          新手引导
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  min-height: 0;
}

.dhead {
  height: 46px;
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2);
  padding: 0 var(--sp-2);
  border-bottom: 1px solid var(--line);
}
.title-row {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  min-width: 0;
}
.p-ico {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  flex: none;
  color: var(--accent);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius);
}
h1 {
  margin: 0;
  font-size: var(--fs-4);
  font-weight: 600;
  letter-spacing: 0.01em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.actions {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex: none;
}

.panels {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 400px;
}
.pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}
.pane-config {
  border-left: 1px solid var(--line);
}
.pane-head {
  height: 34px;
  flex: none;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: var(--sp-1);
  padding: 0 var(--sp-2);
  border-bottom: 1px solid var(--line);
  background: var(--bg-raised);
}
.pane-actions {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: var(--sp-1);
}
.pane-body {
  flex: 1;
  min-height: 0;
}
.pane-body :deep(.el-scrollbar) {
  height: 100%;
}
.muted {
  color: var(--text-faint);
}

.logs-pane {
  flex: none;
  height: 210px;
  border-top: 1px solid var(--line);
  min-height: 0;
}

.detail.empty {
  align-items: center;
  justify-content: center;
  background:
    radial-gradient(80% 120% at 50% 0%, color-mix(in srgb, var(--accent) 6%, transparent), transparent 60%),
    var(--bg-panel);
}
.empty-wrap {
  width: min(430px, 86%);
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 28px;
}
.empty-ico {
  width: 52px;
  height: 52px;
  display: grid;
  place-items: center;
  color: var(--accent);
  border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--line));
  border-radius: var(--radius);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
  margin-bottom: 14px;
}
.empty-wrap h2 {
  margin: 0 0 8px;
  font-size: 17px;
  font-weight: 600;
}
.empty-wrap p {
  margin: 0;
  font-size: var(--fs-2);
  line-height: 1.7;
  color: var(--text-faint);
  max-width: 380px;
}
.empty-actions {
  display: flex;
  gap: 8px;
  margin-top: 18px;
}
@media (max-width: 1200px) {
  .panels {
    grid-template-columns: minmax(0, 1fr) 320px;
  }
}
</style>
