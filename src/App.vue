<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useAppStore } from "@/stores/app";
import TopBar from "@/components/TopBar.vue";
import ProjectRail from "@/components/ProjectRail.vue";
import DetailPanel from "@/components/DetailPanel.vue";
import StatusBar from "@/components/StatusBar.vue";
import NewProjectDialog from "@/components/NewProjectDialog.vue";
import SettingsDialog from "@/components/SettingsDialog.vue";
import OnboardingGuide from "@/components/OnboardingGuide.vue";
import { elementLocale } from "@/i18n";

const store = useAppStore();
const showNew = ref(false);

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
  <el-config-provider :locale="elementLocale">
    <div class="app">
      <TopBar @refresh="refresh" @open-settings="store.openSettings()" />
      <div class="main">
        <ProjectRail @new-project="showNew = true" @open-guide="store.openGuide()" />
        <DetailPanel @new-project="showNew = true" @open-guide="store.openGuide()" />
      </div>
      <StatusBar />
      <OnboardingGuide
        v-if="store.guideOpen"
        @close="store.dismissGuide()"
        @open-settings="(focus) => store.openSettings(focus)"
        @new-project="showNew = true"
      />
      <NewProjectDialog v-model="showNew" />
      <SettingsDialog v-model="store.settingsOpen" :focus="store.settingsFocus" />
    </div>
  </el-config-provider>
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
