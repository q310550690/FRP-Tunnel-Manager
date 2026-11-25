<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "@/stores/app";

const store = useAppStore();
const lines = computed(() => store.logs);

function cls(line: string) {
  if (line.includes("[E]")) return "err";
  if (line.includes("[W]")) return "warn";
  return "";
}
</script>

<template>
  <div class="panel panel-flush logs">

    <div v-for="(l, i) in lines" :key="i" class="line" :class="cls(l)">
      <span class="no mono">{{ String(i + 1).padStart(2, "0") }}</span>
      <span class="text mono">{{ l }}</span>
    </div>
    <div v-if="lines.length === 0" class="empty">暂无日志</div>
  </div>
</template>

<style scoped>
.logs {
  padding: var(--sp-1) 0;
  gap: 0;
}
.line {
  display: flex;
  gap: var(--sp-2);
  padding: 2px var(--sp-2);
  font-size: var(--fs-2);
  line-height: 1.7;
  color: var(--text-dim);
  user-select: text;
}
.line:hover {
  background: var(--bg-hover);
}
.no {
  color: var(--text-faint);
  width: 18px;
  flex: none;
  text-align: right;
  opacity: 0.6;
}
.text {
  white-space: pre-wrap;
  word-break: break-all;
}
.line.err .text {
  color: var(--red);
}
.line.warn .text {
  color: var(--amber);
}
.empty {
  padding: var(--sp-3);
  text-align: center;
  color: var(--text-faint);
  font-size: var(--fs-2);
}
</style>
