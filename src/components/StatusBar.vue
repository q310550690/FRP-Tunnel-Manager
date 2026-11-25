<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { useAppStore } from "@/stores/app";
import AppIcon from "./AppIcon.vue";

const store = useAppStore();

/** 软件版本；浏览器预览时取不到 Tauri API，回落到 package.json 里的版本 */
const appVersion = ref("0.1.0");
onMounted(async () => {
  try {
    appVersion.value = await getVersion();
  } catch {
    /* 非 Tauri 环境下保持默认值 */
  }
});

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
    <span class="item muted" title="FRP Tunnel Manager 软件版本">
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
</style>
