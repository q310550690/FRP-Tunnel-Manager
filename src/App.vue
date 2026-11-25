<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useAppStore } from "@/stores/app";
import TopBar from "@/components/TopBar.vue";
import ProjectRail from "@/components/ProjectRail.vue";
import DetailPanel from "@/components/DetailPanel.vue";
import StatusBar from "@/components/StatusBar.vue";
import NewProjectDialog from "@/components/NewProjectDialog.vue";
import SettingsDialog from "@/components/SettingsDialog.vue";

const store = useAppStore();
const showNew = ref(false);
const showSettings = ref(false);

const mq = window.matchMedia("(prefers-color-scheme: dark)");

function applyTheme() {
  const dark = store.theme === "system" ? mq.matches : store.theme === "dark";
  const root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.setAttribute("data-theme", dark ? "dark" : "light");
}

watch(() => store.theme, applyTheme, { immediate: true });
onMounted(() => {
  applyTheme();
  mq.addEventListener("change", applyTheme);
  // 读取上次保存的项目与设置（<exe>/config/store.json）
  void store.load().then(() => { void store.fetchReleases(); store.startPolling(); });
});
onBeforeUnmount(() => {
  mq.removeEventListener("change", applyTheme);
  store.stopPolling();
});

function refresh() {
  void store.pollOnce();
}
</script>

<template>
  <div class="app">
    <TopBar @refresh="refresh" @open-settings="showSettings = true" />
    <div class="main">
      <ProjectRail @new-project="showNew = true" />
      <DetailPanel />
    </div>
    <StatusBar />
    <NewProjectDialog v-model="showNew" />
    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<style scoped>
.app {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  overflow: hidden;
}
.main {
  flex: 1;
  display: flex;
  min-height: 0;
}
</style>
