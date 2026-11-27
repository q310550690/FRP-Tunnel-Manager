import { t } from ".";
import { templateByKey, type FieldDef, type ServiceState, type Template } from "@/types";
import type { OptimItem } from "@/optimizations";

/** 模板名：优先按 templateKey 查，旧存档只有中文模板名时回退到原名 */
export function templateName(keyOrName?: string, fallback?: string): string {
  const tpl = templateByKey(keyOrName);
  if (tpl) return t(tpl.name);
  return t(keyOrName || fallback || "");
}

export function templateSummary(tpl: Template): string {
  return t(tpl.summary);
}

export function fieldLabel(field: FieldDef): string {
  return t(field.label);
}

export function fieldHint(field: FieldDef): string {
  return field.hint ? t(field.hint) : "";
}

export function fieldPlaceholder(field: FieldDef): string {
  return field.placeholder ? t(field.placeholder) : "";
}

export function optimLabel(item: OptimItem): string {
  return t(item.label);
}

export function optimHint(item: OptimItem): string {
  return t(item.hint);
}

const STATE_KEYS: Record<ServiceState, string> = {
  running: "运行中",
  stopped: "已停止",
  starting: "启动中",
  stopping: "停止中",
  error: "错误",
  "not-installed": "未安装",
};

export function stateLabel(state: ServiceState): string {
  return t(STATE_KEYS[state] ?? state);
}
