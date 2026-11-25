<script setup lang="ts">
import { computed, ref } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";
import { useAppStore } from "@/stores/app";
import type { IconName } from "@/icons";
import type { ServiceState } from "@/types";
import AppIcon from "./AppIcon.vue";
import OverviewTab from "./OverviewTab.vue";
import ConfigTab from "./ConfigTab.vue";
import LogsTab from "./LogsTab.vue";

const store = useAppStore();
const p = computed(() => store.selected);
const busy = ref(false);

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
    if (/管理员/.test(msg)) {
      try {
        await ElMessageBox.confirm(
          "安装 / 卸载 Windows 服务需要管理员权限。是否以管理员身份重新启动本程序？",
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
  if (running.value) void run(() => store.stopProject(p.value!.id), "已停止");
  else void run(() => store.startProject(p.value!.id), "已启动，正在连接服务器…");
}

function restart() {
  if (p.value) void run(() => store.restartProject(p.value!.id), "已重启");
}

function toggleInstall() {
  if (!p.value) return;
  if (installed.value) void run(() => store.uninstallService(p.value!.id), "服务已卸载");
  else void run(() => store.installService(p.value!.id), "已安装为 Windows 服务");
}
</script>

<template>
  <section v-if="p" class="detail">
    <!-- 项目头：唯一一处服务操作入口 -->
    <header class="dhead">
      <div class="title-row">
        <span class="p-ico"><AppIcon :name="iconName" :size="16" /></span>
        <h1>{{ p.name }}</h1>
        <span class="tag">{{ p.template }}</span>
        <span class="tag" :class="{ on: p.state === 'running', err: p.state === 'error' }">
          {{ stateText[p.state] }}
        </span>
        <span v-if="!store.isAdmin" class="tag" title="安装 / 卸载服务需要管理员权限">非管理员</span>
        <span v-if="installed" class="tag on" title="由 Windows 服务托管运行">服务</span>
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
        <div class="pane-body"><el-scrollbar><OverviewTab /></el-scrollbar></div>
      </section>

      <section class="pane pane-config">
        <div class="pane-head">
          <span class="label">配置</span>
          <span class="hint mono">frpc.toml</span>
          <span id="cfg-actions" class="pane-actions"></span>
        </div>
        <div class="pane-body"><el-scrollbar><ConfigTab /></el-scrollbar></div>
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
      <div class="pane-body"><el-scrollbar><LogsTab /></el-scrollbar></div>
    </section>
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

@media (max-width: 1200px) {
  .panels {
    grid-template-columns: minmax(0, 1fr) 320px;
  }
}
</style>
