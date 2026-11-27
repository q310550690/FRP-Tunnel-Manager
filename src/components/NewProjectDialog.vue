<script setup lang="ts">
import { ref } from "vue";
import { TEMPLATES } from "@/types";
import { ElMessage } from "element-plus";
import { PROJECT_COLORS, TEMPLATE_ICONS } from "@/icons";
import AppIcon from "./AppIcon.vue";
import { useAppStore } from "@/stores/app";

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ (e: "update:modelValue", v: boolean): void }>();
const store = useAppStore();
const picked = ref<string | null>(null);
const importing = ref(false);

const idx: Record<string, number> = {};
TEMPLATES.forEach((t, i) => (idx[t.key] = i));

function close() {
  emit("update:modelValue", false);
  picked.value = null;
}

function create() {
  if (!picked.value) return;
  const t = TEMPLATES.find((x) => x.key === picked.value);
  if (!t) return;
  const id = `${t.key}-${Date.now().toString(36)}`;
  const seq = store.projects.length + 1;
  store.projects.push({
    id,
    name: t.name,
    template: t.name,
    templateKey: t.key,
    icon: TEMPLATE_ICONS[t.key] ?? "file",
    color: PROJECT_COLORS[store.projects.length % PROJECT_COLORS.length],
    proxy: { name: `${t.key}-${seq}`, ...JSON.parse(JSON.stringify(t.defaults)) },
    serviceName: `FRP Tunnel · ${t.name}`,
    version: store.defaultVersion || store.versions[0] || "",
    state: "stopped",
    updatedAt: "刚刚",
  });
  store.select(id);
  store.markOnboarded();
  ElMessage({ message: `已创建「${t.name}」，可在配置页调整专属字段`, type: "success", duration: 2200 });
  close();
}

/** 选择已有的 frpc.toml，按其中的代理创建项目；源文件不改动 */
async function importProject() {
  if (importing.value) return;
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    ElMessage.warning("浏览器预览无法选择本机文件，请在桌面应用中使用");
    return;
  }
  importing.value = true;
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      title: "选择 frpc.toml",
      filters: [{ name: "frpc 配置", extensions: ["toml"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    const r = await store.importFrpcToml(selected);
    if (!r.ok) {
      ElMessage.error({ message: `导入失败：${r.reason}`, duration: 6000, showClose: true });
      return;
    }
    const summary =
      r.count === 1 ? `已导入「${r.names?.[0] ?? "项目"}」` : `已导入 ${r.count} 个项目`;
    const warning = r.warnings?.length ? `；${r.warnings.join("；")}` : "";
    ElMessage.success({
      message: summary + warning,
      duration: warning ? 6000 : 2400,
      showClose: warning.length > 0,
    });
    close();
  } catch (e) {
    ElMessage.error({ message: `导入失败：${String(e)}`, duration: 6000, showClose: true });
  } finally {
    importing.value = false;
  }
}
</script>

<template>
  <el-dialog
    :model-value="props.modelValue"
    width="760px"
    :show-close="false"
    align-center
    class="np-dialog"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <template #header>
      <div class="dlg-head">
        <div class="head-text">
          <div class="dlg-title">新建项目</div>
          <div class="dlg-sub">选一个模板，字段和默认端口已经替你填好，之后随时可改。</div>
        </div>
        <button class="btn icon" @click="close">
          <AppIcon name="close" :size="13" />
        </button>
      </div>
    </template>

    <div v-if="!store.hasInstalledFrp" class="np-note">
      <AppIcon name="warn" :size="13" />
      <span>还没有安装 FRP 客户端。项目可以先创建，稍后到「设置 → FRP 版本」安装一个，再回来安装服务。</span>
    </div>

    <div class="tpl-grid">
      <button
        v-for="t in TEMPLATES"
        :key="t.key"
        class="tpl"
        :class="{ on: picked === t.key }"
        :style="{ '--tc': PROJECT_COLORS[idx[t.key] % PROJECT_COLORS.length] }"
        @click="picked = t.key"
      >
        <div class="tpl-top">
          <span class="tpl-ico"><AppIcon :name="TEMPLATE_ICONS[t.key]" :size="16" /></span>
          <span class="tpl-name">{{ t.name }}</span>
          <span class="tag">{{ t.type }}</span>
        </div>
        <div class="tpl-desc">{{ t.summary }}</div>
        <div class="tpl-def mono">
          127.0.0.1:{{ t.defaults.localPort }}
          <template v-if="t.defaults.remotePort"> → {{ t.defaults.remotePort }}</template>
          <template v-else-if="t.defaults.customDomain"> → {{ t.defaults.customDomain }}</template>
        </div>
      </button>
    </div>

    <template #footer>
      <div class="dlg-foot">
        <span class="hint">{{ picked ? "已选择模板" : "请选择一个模板" }}</span>
        <div class="btns">
          <button class="btn" @click="close">取消</button>
          <button class="btn" :disabled="importing" @click="importProject">
            <AppIcon name="up" :size="12" />{{ importing ? "导入中…" : "导入项目" }}
          </button>
          <button class="btn accent" :disabled="!picked" @click="create">创建项目</button>
        </div>
      </div>
    </template>
  </el-dialog>
</template>

<style scoped>
.dlg-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--sp-2);
}
.head-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.dlg-title {
  font-size: var(--fs-4);
  font-weight: 600;
}
.dlg-sub {
  font-size: var(--fs-2);
  color: var(--text-faint);
}
.np-note {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-bottom: var(--sp-2);
  padding: 8px 10px;
  font-size: var(--fs-2);
  line-height: 1.55;
  color: color-mix(in srgb, var(--amber) 85%, var(--text));
  border: 1px solid color-mix(in srgb, var(--amber) 35%, var(--line));
  background: color-mix(in srgb, var(--amber) 8%, transparent);
  border-radius: var(--radius);
}
.np-note :deep(.app-icon) {
  margin-top: 1px;
}

.tpl-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--sp-1);
}
.tpl {
  text-align: left;
  padding: var(--sp-2);
  background: var(--bg-raised);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  color: var(--text-dim);
  font-family: inherit;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
  transition: border-color 0.1s ease, background-color 0.1s ease;
}
.tpl:hover {
  border-color: var(--line-accent);
  background: var(--bg-hover);
}
.tpl.on {
  border-color: var(--tc);
  border-left-color: var(--tc);
  background: color-mix(in srgb, var(--tc) 8%, var(--bg-raised));
}
.tpl-top {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
  color: var(--text);
}
.tpl-ico {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  flex: none;
  color: var(--tc);
  border: 1px solid color-mix(in srgb, var(--tc) 28%, transparent);
  background: color-mix(in srgb, var(--tc) 8%, transparent);
  border-radius: var(--radius);
}
.tpl-name {
  font-size: var(--fs-2);
  font-weight: 500;
}
.tpl-top .tag {
  margin-left: auto;
}
.tpl-desc {
  font-size: var(--fs-1);
  line-height: 1.5;
  color: var(--text-faint);
  min-height: 30px;
}
.tpl-def {
  font-size: var(--fs-1);
  color: var(--text-dim);
}

.dlg-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2);
}
.hint {
  font-size: var(--fs-2);
  color: var(--text-faint);
}
.btns {
  display: flex;
  gap: var(--sp-1);
}
</style>
