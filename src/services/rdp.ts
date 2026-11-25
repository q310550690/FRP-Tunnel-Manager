import { invoke } from "@tauri-apps/api/core";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** 读取当前 Windows 远程桌面组策略值（未配置的键不会出现在结果里） */
export async function rdpGet(): Promise<Record<string, number>> {
  if (!isTauri) return {};
  try {
    return await invokeCmd<Record<string, number>>("rdp_policy_get");
  } catch {
    return {};
  }
}

/** 写入/清除一条策略：value=null 表示恢复「未配置」 */
export async function rdpSet(name: string, value: number | null): Promise<void> {
  if (!isTauri) return;
  await invokeCmd<void>("rdp_policy_set", { name, value });
}

export { isTauri };
