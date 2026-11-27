<script setup lang="ts">
import { computed } from "vue";
import { useAppStore, type SettingsFocus } from "@/stores/app";
import AppIcon from "@/components/AppIcon.vue";

const store = useAppStore();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "open-settings", focus: SettingsFocus): void;
  (e: "new-project"): void;
}>();

const serverDone = computed(() => !!store.server.addr.trim());
const frpDone = computed(() => store.hasInstalledFrp);
const projectDone = computed(() => store.projects.length > 0);
const doneCount = computed(
  () => [serverDone.value, frpDone.value, projectDone.value].filter(Boolean).length
);

const steps = computed(() => [
  {
    key: "server",
    title: "配置服务器",
    desc: "填上 frps 的地址和端口，所有项目默认走这台服务器。",
    done: serverDone.value,
    action: "去配置",
    focus: "server" as SettingsFocus,
  },
  {
    key: "frp",
    title: "安装 FRP 客户端",
    desc: "从 GitHub 下载 frpc 到程序目录，或导入本机已有的 frpc，便携存放，不写系统目录。",
    done: frpDone.value,
    action: "去安装",
    focus: "frp" as SettingsFocus,
  },
  {
    key: "project",
    title: "新建一个项目",
    desc: "选择模板、填好本地端口，启动后就能从公网访问。",
    done: projectDone.value,
    action: "新建项目",
    focus: null,
  },
]);

function act(step: (typeof steps.value)[number]) {
  if (step.key === "project") emit("new-project");
  else emit("open-settings", step.focus);
}

function finish() {
  store.dismissGuide();
  emit("close");
}
</script>

<template>
  <div class="guide" role="dialog" aria-modal="true" aria-labelledby="guide-title">
    <div class="guide-card">
      <header class="guide-head">
        <div class="mark" aria-hidden="true">
          <AppIcon name="bolt" :size="20" />
        </div>
        <div class="head-text">
          <span class="eyebrow mono">GETTING STARTED</span>
          <h1 id="guide-title">三步把隧道跑起来</h1>
          <p>FRP Tunnel Manager 把 frpc、配置和日志都放在程序目录里，随拷随用。</p>
        </div>
        <div class="progress mono" :title="`已完成 ${doneCount} / 3`">
          <strong>{{ doneCount }}</strong><span>/ 3</span>
        </div>
      </header>

      <ol class="steps">
        <li v-for="(step, i) in steps" :key="step.key" class="step" :class="{ done: step.done }">
          <span class="idx mono">
            <AppIcon v-if="step.done" name="ok" :size="13" />
            <template v-else>0{{ i + 1 }}</template>
          </span>
          <div class="step-body">
            <div class="step-title">
              {{ step.title }}
              <span v-if="step.done" class="tag on">已完成</span>
            </div>
            <p>{{ step.desc }}</p>
          </div>
          <button class="btn" :class="{ accent: !step.done }" @click="act(step)">
            {{ step.done ? (step.key === "project" ? "再建一个" : "重新设置") : step.action }}
            <AppIcon name="chevron" :size="11" />
          </button>
        </li>
      </ol>

      <footer class="guide-foot">
        <span class="hint">
          这些设置以后都能在右上角
          <AppIcon name="settings" :size="11" class="inline-ico" />
          齿轮里修改
        </span>
        <div class="foot-actions">
          <button class="btn" @click="finish">跳过引导</button>
          <button class="btn solid" @click="finish">开始使用</button>
        </div>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.guide {
  position: fixed;
  left: 0;
  right: 0;
  top: 0;
  bottom: 0;
  z-index: 900;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(10, 14, 19, 0.58);
  backdrop-filter: blur(3px);
  animation: guide-fade 0.22s ease-out;
}

.guide-card {
  width: min(720px, 100%);
  max-height: 100%;
  overflow: auto;
  background: var(--bg-panel);
  border: 1px solid var(--line);
  border-radius: calc(var(--radius) + 2px);
  box-shadow: 0 24px 70px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(255, 255, 255, 0.02) inset;
  animation: guide-rise 0.26s cubic-bezier(0.2, 0.7, 0.2, 1);
}

.guide-head {
  display: flex;
  gap: 14px;
  align-items: flex-start;
  padding: 20px 22px 18px;
  border-bottom: 1px solid var(--line);
  background:
    radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--accent) 12%, transparent), transparent 55%),
    var(--bg-panel);
}

.mark {
  width: 38px;
  height: 38px;
  flex: none;
  display: grid;
  place-items: center;
  color: var(--accent);
  border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--line));
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border-radius: var(--radius);
}

.head-text {
  min-width: 0;
  flex: 1;
}
.eyebrow {
  font-size: var(--fs-1);
  letter-spacing: 0.18em;
  color: var(--text-faint);
}
.head-text h1 {
  margin: 4px 0 4px;
  font-size: 19px;
  font-weight: 650;
  letter-spacing: 0.01em;
}
.head-text p {
  margin: 0;
  font-size: var(--fs-2);
  color: var(--text-faint);
  line-height: 1.6;
}
.progress {
  flex: none;
  display: flex;
  align-items: baseline;
  gap: 2px;
  padding: 5px 9px;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--text-faint);
  font-size: var(--fs-1);
}
.progress strong {
  color: var(--accent);
  font-size: var(--fs-3);
}

.steps {
  list-style: none;
  margin: 0;
  padding: 10px 22px;
}
.step {
  display: flex;
  align-items: center;
  gap: 13px;
  padding: 13px 0;
  border-bottom: 1px dashed color-mix(in srgb, var(--line) 70%, transparent);
}
.step:last-child {
  border-bottom: 0;
}
.idx {
  width: 28px;
  height: 28px;
  flex: none;
  display: grid;
  place-items: center;
  border: 1px solid var(--line);
  border-radius: 50%;
  color: var(--text-faint);
  font-size: var(--fs-1);
}
.step.done .idx {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.step-body {
  flex: 1;
  min-width: 0;
}
.step-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--fs-3);
  font-weight: 600;
}
.step-body p {
  margin: 3px 0 0;
  font-size: var(--fs-2);
  color: var(--text-faint);
  line-height: 1.55;
}
.step .btn {
  flex: none;
}

.guide-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 22px;
  border-top: 1px solid var(--line);
  background: color-mix(in srgb, var(--bg) 55%, var(--bg-panel));
}
.hint {
  font-size: var(--fs-2);
  color: var(--text-faint);
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.inline-ico {
  display: inline-block;
  vertical-align: -1px;
}
.foot-actions {
  display: flex;
  gap: 8px;
}

@keyframes guide-fade {
  from {
    opacity: 0;
  }
}
@keyframes guide-rise {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.99);
  }
}
@media (prefers-reduced-motion: reduce) {
  .guide,
  .guide-card {
    animation: none;
  }
}
</style>
