<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "@/stores/app";
import { effectiveServer } from "@/utils/toml";
import type { IconName } from "@/icons";
import { PROJECT_COLORS } from "@/icons";
import AppIcon from "./AppIcon.vue";
import LedDot from "./LedDot.vue";
import type { Project, ServiceState } from "@/types";

const store = useAppStore();
const emit = defineEmits<{ (e: "new-project"): void }>();

const stateText: Record<ServiceState, string> = {
  running: "运行中",
  stopped: "已停止",
  starting: "启动中",
  stopping: "停止中",
  error: "错误",
  "not-installed": "未安装",
};

function iconOf(p: Project): IconName {
  return (p.icon as IconName) ?? "file";
}

function colorOf(p: Project, index: number): string {
  return p.color ?? PROJECT_COLORS[index % PROJECT_COLORS.length];
}

function endpoint(p: Project) {
  const addr = effectiveServer(p, store.server).addr;
  if (p.proxy.type === "http" || p.proxy.type === "https") return p.proxy.customDomain ?? "—";
  return p.proxy.remotePort ? `${addr}:${p.proxy.remotePort}` : "无公网端口";
}

const list = computed(() => store.filtered);
</script>

<template>
  <aside class="rail">
    <div class="rail-head">
      <label class="search">
        <AppIcon name="search" :size="12" />
        <input v-model="store.query" type="text" placeholder="搜索项目、模板或端口" spellcheck="false" />
        <button v-if="store.query" class="clear" title="清空" @click="store.query = ''">
          <AppIcon name="close" :size="11" />
        </button>
      </label>
      <button class="btn icon solid" title="新建项目" @click="emit('new-project')">
        <AppIcon name="plus" :size="16" />
      </button>
    </div>

    <el-scrollbar class="rail-scroll">
      <div class="rail-list">
      <button
        v-for="(p, i) in list"
        :key="p.id"
        class="item"
        :class="{ active: p.id === store.selectedId }"
        :style="{ '--pc': colorOf(p, i) }"
        @click="store.select(p.id)"
      >
        <span class="ico"><AppIcon :name="iconOf(p)" :size="16" /></span>
        <div class="meta">
          <div class="line-1">
            <span class="name">{{ p.name }}</span>
            <span class="state">
              <LedDot :state="p.state" :size="6" :pulse="false" />
              {{ stateText[p.state] }}
            </span>
          </div>
          <div class="line-2">
            <span class="proto mono">{{ p.proxy.type }}</span>
            <span class="sep">·</span>
            <span class="ep mono">{{ endpoint(p) }}</span>
          </div>
        </div>
      </button>

      <div v-if="list.length === 0" class="empty">
        <AppIcon name="inbox" :size="18" />
        <span>{{ store.query ? `没有匹配「${store.query}」的项目` : "该状态下没有项目" }}</span>
      </div>
      </div>
    </el-scrollbar>
  </aside>
</template>

<style scoped>
.rail {
  width: var(--w-rail);
  flex: none;
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  border-right: 1px solid var(--line);
  min-height: 0;
}

.rail-head {
  height: 46px;
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 0 var(--sp-2);
  border-bottom: 1px solid var(--line);
}

.search {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  height: 26px;
  padding: 0 var(--sp-1);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  color: var(--text-faint);
  transition: border-color 0.15s ease;
}
.search:focus-within {
  border-color: var(--accent);
}
.search input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 0;
  outline: 0;
  color: var(--text);
  font-family: inherit;
  font-size: var(--fs-2);
}
.search input::placeholder {
  color: var(--text-faint);
}
.clear {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  background: transparent;
  border: 0;
  color: var(--text-faint);
  cursor: pointer;
}
.clear:hover {
  color: var(--text);
}

/* 4) 列表加内边距 */
.rail-scroll {
  flex: 1;
  min-height: 0;
}
.rail-list {
  padding: var(--sp-2);
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.item {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  width: 100%;
  padding: var(--sp-1) var(--sp-2);
  text-align: left;
  background: transparent;
  border: 1px solid transparent;
  border-left: 2px solid transparent;
  border-radius: var(--radius);
  color: var(--text-dim);
  font-family: inherit;
  cursor: pointer;
  transition: background-color 0.15s ease, border-color 0.15s ease;
}
.item:hover {
  background: var(--bg-hover);
}
.item.active {
  background: var(--bg-raised);
  border-color: var(--line);
  border-left-color: var(--pc);
}
.ico {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  flex: none;
  color: var(--pc);
  border: 1px solid color-mix(in srgb, var(--pc) 26%, transparent);
  background: color-mix(in srgb, var(--pc) 8%, transparent);
  border-radius: var(--radius);
}
.meta {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.line-1 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-1);
  min-width: 0;
  height: 16px;
  line-height: 16px;
}
.state {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  flex: none;
  font-size: var(--fs-1);
  color: var(--text-faint);
  white-space: nowrap;
}
.name {
  min-width: 0;
  font-size: var(--fs-3);
  font-weight: 500;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.line-2 {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: var(--fs-1);
  color: var(--text-faint);
  min-width: 0;
  height: 12px;
  line-height: 12px;
}
.line-2 .proto {
  flex: none;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.line-2 .sep {
  opacity: 0.5;
}
.ep {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-1);
  padding: var(--sp-3);
  text-align: center;
  font-size: var(--fs-2);
  color: var(--text-faint);
}
</style>
