<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "@/stores/app";
import { effectiveServer } from "@/utils/toml";
import AppIcon from "./AppIcon.vue";
import LedDot from "./LedDot.vue";

const store = useAppStore();
const p = computed(() => store.selected!);
const srv = computed(() => (p.value ? effectiveServer(p.value, store.server) : store.server));
const tunnel = computed(() => (p.value ? store.tunnelOk[p.value.id] : null));

const address = computed(() => {
  if (!p.value) return "—";
  const t = p.value.proxy.type;
  if (t === "http" || t === "https") return `https://${p.value.proxy.customDomain ?? "—"}`;
  if (t === "stcp" || t === "xtcp") return "点对点（无需公网端口）";
  return `${srv.value.addr}:${p.value.proxy.remotePort ?? "—"}`;
});

function copy() {
  navigator.clipboard?.writeText(address.value);
}

/** 服务标识：Windows 是服务名，macOS 是 launchd Label（com.sxq.frptm.<id>） */
const serviceId = computed(() => {
  if (!p.value) return "—";
  const prefix = store.platformInfo.labelPrefix;
  return prefix ? `${prefix}${p.value.id}` : p.value.serviceName;
});

/** 真实事件：取当前项目最近 5 条 frpc 日志 */
const events = computed(() => {
  const lines = (p.value ? store.logs : []) as string[];
  return lines
    .slice(-5)
    .reverse()
    .map((raw) => {
      const m = raw.match(/^(\d{2}):(\d{2}):(\d{2})[^\[]*\[[^\]]*\]\s*(.*)$/);
      if (m) return { t: `${m[1]}:${m[2]}:${m[3]}`, text: m[4] };
      return { t: "", text: raw };
    });
});
</script>

<template>
  <div v-if="p" class="panel">
    <Teleport to="#ov-actions" defer>
      <button class="btn sm accent" title="复制连接地址" @click="copy">
        <AppIcon name="copy" :size="12" />
        复制连接
      </button>
    </Teleport>
    <div class="addr-block">
      <div class="label">连接地址</div>
      <div class="addr-row">
        <LedDot :state="p.state" :size="8" />
        <span class="addr mono">{{ address }}</span>
      </div>
      <div class="hint">mstsc / 浏览器 / 客户端都连这里；地址随项目配置实时变化。</div>
    </div>

    <div class="grid">
      <div class="cell">
        <span class="label">{{ store.platformInfo.serviceIdLabel }}</span>
        <span class="mono val">{{ serviceId }}</span>
      </div>
      <div class="cell">
        <span class="label">frp 版本</span>
        <span class="mono val">{{ p.version }}</span>
      </div>
      <div class="cell">
        <span class="label">本地监听</span>
        <span class="mono val">{{ p.proxy.localIP }}:{{ p.proxy.localPort }}</span>
      </div>
      <div class="cell">
        <span class="label">进程 PID</span>
        <span class="mono val">{{ p.pid ?? "—" }}</span>
      </div>
      <div class="cell">
        <span class="label">运行时长</span>
        <span class="mono val">{{ p.uptime ?? "—" }}</span>
      </div>
      <div class="cell">
        <span class="label">最后修改</span>
        <span class="mono val">{{ p.updatedAt }}</span>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">流量</span>
        <span class="hint mono">未接入 frp 指标</span>
      </div>
      <div class="t-row">
        <AppIcon name="down" :size="12" />
        <span class="t-name">下行</span>
        <span class="t-bar"><i style="width: 0" /></span>
        <b class="mono t-val">—</b>
      </div>
      <div class="t-row">
        <AppIcon name="up" :size="12" />
        <span class="t-name">上行</span>
        <span class="t-bar"><i style="width: 0" /></span>
        <b class="mono t-val">—</b>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">隧道细节</span>
        <span class="hint mono">{{ p.proxy.name }}</span>
      </div>
      <div class="tunnel-body">
        <div class="kv"><span>代理类型</span><b class="mono up">{{ p.proxy.type }}</b></div>
        <div class="kv"><span>公网端口</span><b class="mono">{{ p.proxy.remotePort ?? "—" }}</b></div>
        <div class="kv"><span>服务器</span><b class="mono">{{ srv.addr }}:{{ srv.port }}</b></div>
        <div class="kv">
          <span>传输</span>
          <b class="mono">{{ srv.protocol.toUpperCase() }}{{ srv.tls ? " · TLS" : "" }}</b>
        </div>
        <div class="kv">
          <span>隧道状态</span>
          <b :class="{ ok: tunnel === true, bad: tunnel === false }">
            <LedDot :state="tunnel === true ? 'running' : tunnel === false ? 'error' : 'stopped'" :size="8" />
            {{ tunnel === true ? "已连接" : tunnel === false ? "未连接" : "未知" }}
          </b>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">最近事件</span>
        <span class="hint mono">来自 frpc 日志</span>
      </div>
      <div v-if="events.length === 0" class="tl-empty muted">暂无事件，启动后这里显示 frpc 的最新日志。</div>
      <div v-for="(e, i) in events" :key="i" class="tl-row">
        <span class="tl-time mono">{{ e.t }}</span>
        <span class="tl-led" data-lvl="ok" />
        <span class="tl-text">{{ e.text }}</span>
      </div>
    </div>

    <div v-if="p.state === 'error'" class="err-strip">
      <AppIcon name="warn" :size="14" />
      最近一次启动失败：<span class="mono">port already used</span>（remote_port 6010 已被占用），换一个端口或停止冲突项目后重试。
    </div>
    <div v-else class="ok-strip">
      <AppIcon name="shield" :size="14" />
      隧道由本机 {{ store.platformInfo.serviceKind }} 托管，{{ store.platformInfo.autostart }}；异常退出会按 3 秒间隔重试。
    </div>
  </div>
</template>

<style scoped>
.addr-block {
  border: 1px solid var(--line);
  background: var(--bg-raised);
  padding: var(--sp-2);
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
}
.addr-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}
.addr {
  font-size: var(--fs-5);
  letter-spacing: 0.02em;
  color: var(--text);
}

.grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  border-top: 1px solid var(--line);
  border-left: 1px solid var(--line);
}
.cell {
  background: var(--bg-panel);
  border-right: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
  padding: var(--sp-1) var(--sp-2);
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
/* 行高取整，避免 1px 分隔线落在半像素上被渲染成 2px */
.cell > span {
  line-height: 16px;
}
.val {
  font-size: var(--fs-2);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.t-row {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  padding: var(--sp-1) var(--sp-2);
  font-size: var(--fs-1);
  color: var(--text-faint);
}
.t-row + .t-row {
  border-top: 1px solid var(--line);
}
.t-name {
  width: 28px;
  flex: none;
  color: var(--text-dim);
}
.t-bar {
  flex: 1;
  height: 4px;
  background: var(--bg-hover);
  border: 1px solid var(--line);
}
.t-bar i {
  display: block;
  height: 100%;
  background: var(--accent);
  opacity: 0.8;
}
.t-val {
  min-width: 64px;
  text-align: right;
  font-size: var(--fs-1);
  color: var(--text);
}

.tunnel-body {
  display: grid;
  grid-template-columns: 1fr 1fr;
}
.kv {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--sp-1) var(--sp-2);
  font-size: var(--fs-2);
  color: var(--text-faint);
  border-bottom: 1px solid var(--line);
}
.kv:nth-child(odd) {
  border-right: 1px solid var(--line);
}
.kv:nth-last-child(-n + 2) {
  border-bottom: none;
}
.kv b {
  font-weight: 500;
  color: var(--text);
  font-size: var(--fs-2);
}
.kv b.ok,
.kv b.bad {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-family: inherit;
}
.kv b.ok {
  color: var(--accent);
}
.kv b.bad {
  color: var(--red);
}

.tl-empty {
  padding: var(--sp-2);
  font-size: var(--fs-2);
}

.kv b.up {
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.tl-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-1) var(--sp-2);
  font-size: var(--fs-2);
  color: var(--text-dim);
}
.tl-row + .tl-row {
  border-top: 1px solid var(--line);
}
.tl-time {
  width: 74px;
  flex: none;
  font-size: var(--fs-1);
  color: var(--text-faint);
}
.tl-led {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--text-faint);
  flex: none;
}
.tl-led[data-lvl="ok"] {
  background: var(--accent);
  box-shadow: 0 0 5px 0 color-mix(in srgb, var(--accent) 50%, transparent);
}
.tl-led[data-lvl="warn"] {
  background: var(--amber);
}
.tl-led[data-lvl="err"] {
  background: var(--red);
}
.tl-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ok-strip,
.err-strip {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  padding: var(--sp-1) var(--sp-2);
  border: 1px solid var(--line);
  font-size: var(--fs-2);
  color: var(--text-dim);
}
.ok-strip {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.err-strip {
  border-color: var(--red-line);
  background: var(--red-soft);
  color: var(--text);
}
</style>
