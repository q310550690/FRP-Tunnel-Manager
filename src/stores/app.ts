import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import type { AppPaths, FrpVersion, Project, ServerConfig, ServiceState, TransportConfig } from "@/types";
import { GLOBAL_SERVER, MOCK_PROJECTS } from "@/mock/data";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { adminPort, buildToml } from "@/utils/toml";

export type FilterKey = "all" | "running" | "stopped" | "error";
export type ThemeMode = "system" | "dark" | "light";
export type { ServerConfig };

export interface NetworkConfig {
  useProxy: boolean;
  proxyUrl: string;
  /** 下载镜像前缀，例如 https://gh-proxy.com/（直连 GitHub 超时时用） */
  mirror: string;
}

export interface ProcInfo {
  running: boolean;
  pid?: number | null;
  uptime: number;
}

interface VersionProgressPayload {
  version: string;
  phase: string;
  downloaded: number;
  total: number;
}

export const useAppStore = defineStore("app", () => {
  const projects = ref<Project[]>(MOCK_PROJECTS);
  const selectedId = ref<string>(MOCK_PROJECTS[0].id);
  const filter = ref<FilterKey>("all");
  const query = ref("");
  const theme = ref<ThemeMode>("system");
  const logAutoScroll = ref(true);
  const server = ref<ServerConfig>({ ...GLOBAL_SERVER });
  const globalTransport = ref<TransportConfig>({ tcpMux: true, poolCount: 0, heartbeatInterval: 30 });
  function updateTransport(next: Partial<TransportConfig>) {
    globalTransport.value = { ...globalTransport.value, ...next };
  }

  const network = ref<NetworkConfig>({
    useProxy: false,
    proxyUrl: "http://127.0.0.1:7890",
    mirror: "https://gh-proxy.com/",
  });
  const paths = ref<AppPaths>({
    projects: "./projects",
    config: "./config/app.json",
  });

  /** 版本仓库：已安装 / 未安装，全局默认版本只是新建项目的预填值 */
  const versionList = ref<FrpVersion[]>([
    { version: "0.61.1", installed: false },
    { version: "0.60.0", installed: false },
    { version: "0.52.3", installed: false },
  ]);
  const defaultVersion = ref("0.61.1");

  const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  interface PersistShape {
    version: number;
    projects: Project[];
    selectedId: string;
    server: ServerConfig;
    transport: TransportConfig;
    network: NetworkConfig;
    paths: AppPaths;
    defaultVersion: string;
    theme: ThemeMode;
  }

  function snapshot(): PersistShape {
    return {
      version: 1,
      projects: projects.value,
      selectedId: selectedId.value,
      server: server.value,
      transport: globalTransport.value,
      network: network.value,
      paths: paths.value,
      defaultVersion: defaultVersion.value,
      theme: theme.value,
    };
  }

  // ───────────────────────── 运行时状态（不落盘） ─────────────────────────
  const projectLogs = ref<Record<string, string[]>>({});
  const tunnelOk = ref<Record<string, boolean>>({});

  const isAdmin = ref(false);

  async function checkAdmin() {
    if (!inTauri()) return;
    try {
      isAdmin.value = await invoke<boolean>("is_elevated");
    } catch {
      isAdmin.value = false;
    }
  }

  async function relaunchElevated() {
    if (!inTauri()) return;
    await invoke("relaunch_elevated");
  }
  const procPid = ref<Record<string, number>>({});
  const installProgress = ref<Record<string, { phase: string; pct: number }>>({});
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let eventsBound = false;
  let persisting = false;

  const nowText = () => new Date().toLocaleTimeString("zh-CN", { hour12: false });

  function fmtSize(bytes: number) {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  function pushLog(id: string, line: string) {
    const arr = projectLogs.value[id] ?? (projectLogs.value[id] = []);
    arr.push(line);
    if (arr.length > 800) arr.splice(0, arr.length - 800);
  }

  /** 绑定 Rust 侧事件（只绑定一次） */
  function bindEvents() {
    if (eventsBound || !inTauri()) return;
    eventsBound = true;
    void listen<{ id: string; line: string }>("frpc-log", (e) => {
      pushLog(e.payload.id, e.payload.line);
      const p = projects.value.find((x) => x.id === e.payload.id);
      if (p) p.updatedAt = "刚刚";
    });
    void listen<{ id: string }>("frpc-exit", (e) => {
      const id = e.payload.id;
      const p = projects.value.find((x) => x.id === id);
      if (p && p.state !== "stopped") {
        p.state = "stopped";
        p.pid = undefined;
        p.uptime = undefined;
        tunnelOk.value[id] = false;
      }
      delete procPid.value[id];
    });
    void listen<VersionProgressPayload>("version-progress", (e) => {
      const { version, phase, downloaded, total } = e.payload;
      const pct = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
      installProgress.value[version] = { phase, pct };
    });
  }

  /** 从 <exe>/config/store.json 读取上次的状态；没有文件就保持默认 */
  async function load() {
    if (!inTauri()) return;
    try {
      const text = await invoke<string>("read_store");
      if (text && text.trim()) {
        const data = JSON.parse(text) as Partial<PersistShape>;
        if (Array.isArray(data.projects) && data.projects.length) {
          projects.value = data.projects.map((p) => {
            const raw = p as unknown as { state: string; serviceInstalled?: boolean };
            if (raw.state === "not-installed") {
              return { ...p, state: "stopped" as Project["state"], serviceInstalled: false };
            }
            return { ...p, serviceInstalled: raw.serviceInstalled === true };
          });
          selectedId.value =
            data.projects.some((p) => p.id === data.selectedId) && data.selectedId
              ? data.selectedId
              : data.projects[0].id;
        }
        if (data.server) server.value = { ...server.value, ...data.server };
        if (data.transport) globalTransport.value = { ...globalTransport.value, ...data.transport };
        if (data.network) network.value = { ...network.value, ...data.network };
        if (data.paths) paths.value = { ...paths.value, ...data.paths };
        if (data.defaultVersion) defaultVersion.value = data.defaultVersion;
        if (data.theme) theme.value = data.theme;
      }
    } catch (e) {
      console.warn("读取本地数据失败：", e);
    }
    bindEvents();
    await refreshLocalVersions();
    void checkAdmin();
    void ensureLogs(selectedId.value);
  }

  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  /** 防抖写盘，任何项目/设置变动都会落盘 */
  function persist() {
    if (!inTauri() || persisting) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persisting = true;
      void invoke<string>("write_store", { json: JSON.stringify(snapshot()) })
        .catch((e) => console.warn("保存本地数据失败：", e))
        .finally(() => {
          persisting = false;
        });
    }, 300);
  }

  watch([projects, server, globalTransport, network, paths, defaultVersion, theme], persist, { deep: true });

  const releaseLoading = ref(false);
  const releaseError = ref("");

  /** GitHub 拉不到时的兜底列表（都是真实发布过的版本） */
  const FALLBACK_RELEASES = [
    "0.62.0", "0.61.1", "0.61.0", "0.60.0", "0.59.0",
    "0.58.0", "0.57.0", "0.56.0", "0.55.0", "0.54.0",
    "0.53.0", "0.52.3", "0.52.2",
  ];

  function cmpVersion(a: string, b: string) {
    const pa = a.split(".").map(Number);
    const pb = b.split(".").map(Number);
    for (let i = 0; i < 3; i++) {
      const d = (pa[i] ?? 0) - (pb[i] ?? 0);
      if (d !== 0) return d;
    }
    return 0;
  }

  /** 读本地 versions/ 目录，把「已安装」状态同步到列表 */
  async function refreshLocalVersions() {
    if (!inTauri()) return;
    try {
      const local = await invoke<{ version: string; size: number }[]>("version_list_local");
      const map = new Map(local.map((l) => [l.version, l.size]));
      for (const v of versionList.value) {
        v.installed = map.has(v.version);
        v.size = map.has(v.version) ? fmtSize(map.get(v.version) as number) : undefined;
      }
      for (const l of local) {
        if (!versionList.value.some((v) => v.version === l.version)) {
          versionList.value.push({ version: l.version, installed: true, size: fmtSize(l.size) });
        }
      }
      versionList.value.sort((a, b) => cmpVersion(b.version, a.version));
    } catch (e) {
      console.warn("读取本地版本失败：", e);
    }
  }

  /** 从 GitHub Releases 拉真实版本列表，失败时退回内置列表 */
  async function fetchReleases() {
    if (releaseLoading.value) return;
    releaseLoading.value = true;
    releaseError.value = "";
    await refreshLocalVersions();
    const installed = new Map(
      versionList.value.filter((v) => v.installed).map((v) => [v.version, v])
    );
    let list: FrpVersion[] = [];
    try {
      const url = "https://api.github.com/repos/fatedier/frp/releases?per_page=40";
      let raw: { tag_name: string; prerelease?: boolean }[];
      if (inTauri()) {
        // 走 Rust 侧请求：支持「设置 → 网络代理」里配置的 HTTP 代理
        const text = await invoke<string>("http_get_text", {
          url,
          proxy: network.value.useProxy ? network.value.proxyUrl : null,
        });
        raw = JSON.parse(text);
      } else {
        const ctrl = new AbortController();
        const timer = setTimeout(() => ctrl.abort(), 6000);
        const res = await fetch(url, {
          signal: ctrl.signal,
          headers: { Accept: "application/vnd.github+json" },
        });
        clearTimeout(timer);
        if (!res.ok) throw new Error("HTTP " + res.status);
        raw = (await res.json()) as { tag_name: string; prerelease?: boolean }[];
      }
      list = raw
        .filter((r) => !r.prerelease)
        .map((r) => r.tag_name.replace(/^v/, ""))
        .filter((v) => /^\d+\.\d+\.\d+$/.test(v) && cmpVersion(v, "0.52.0") >= 0)
        .map((v) => ({ version: v, installed: installed.has(v), size: installed.get(v)?.size }));
    } catch {
      releaseError.value = "GitHub 不可达，显示内置版本列表";
    }
    if (list.length === 0) {
      list = FALLBACK_RELEASES.map((v) => ({
        version: v,
        installed: installed.has(v),
        size: installed.get(v)?.size,
      }));
    }
    for (const v of installed.values()) {
      if (!list.some((x) => x.version === v.version)) list.push({ ...v });
    }
    list.sort((a, b) => cmpVersion(b.version, a.version));
    versionList.value = list;
    if (!versionList.value.some((v) => v.version === defaultVersion.value)) {
      const first = versionList.value.find((v) => v.installed);
      if (first) defaultVersion.value = first.version;
    }
    releaseLoading.value = false;
  }

  const versions = computed(() =>
    versionList.value.filter((v) => v.installed).map((v) => v.version)
  );

  const selected = computed<Project | undefined>(() =>
    projects.value.find((p) => p.id === selectedId.value)
  );

  const logs = computed<string[]>(() => projectLogs.value[selectedId.value] ?? []);

  const counts = computed(() => {
    const base = { running: 0, stopped: 0, error: 0, total: projects.value.length };
    for (const p of projects.value) {
      if (p.state === "running" || p.state === "starting") base.running++;
      else if (p.state === "error") base.error++;
      else base.stopped++;
    }
    return base;
  });

  /** 状态筛选 + 关键字搜索（名称 / 服务名 / 代理名 / 模板） */
  const filtered = computed<Project[]>(() => {
    let list = projects.value;
    if (filter.value === "running")
      list = list.filter((p) => p.state === "running" || p.state === "starting");
    else if (filter.value === "error") list = list.filter((p) => p.state === "error");
    else if (filter.value === "stopped")
      list = list.filter((p) => p.state === "stopped" || p.state === "not-installed");

    const q = query.value.trim().toLowerCase();
    if (q) {
      list = list.filter((p) =>
        (p.name + " " + p.serviceName + " " + p.proxy.name + " " + p.template)
          .toLowerCase()
          .includes(q)
      );
    }
    return list;
  });

  function select(id: string) {
    selectedId.value = id;
    void ensureLogs(id);
  }

  /** 进程没跑时从磁盘补历史日志，保证「最近事件」不是空的 */
  async function ensureLogs(id: string) {
    if (!inTauri() || projectLogs.value[id]?.length) return;
    try {
      const lines = await invoke<string[]>("project_read_log", { id });
      if (lines.length) projectLogs.value[id] = lines;
    } catch {
      /* 没有日志文件就算了 */
    }
  }

  function setFilter(f: FilterKey) {
    filter.value = filter.value === f ? "all" : f;
  }

  function setState(id: string, state: ServiceState, extra: Partial<Project> = {}) {
    const p = projects.value.find((x) => x.id === id);
    if (p) Object.assign(p, { state, ...extra });
  }

  function cycleTheme() {
    theme.value = theme.value === "system" ? "dark" : theme.value === "dark" ? "light" : "system";
  }

  function toggleLogAutoScroll() {
    logAutoScroll.value = !logAutoScroll.value;
  }

  function setTheme(mode: ThemeMode) {
    theme.value = mode;
  }

  function updateServer(next: ServerConfig) {
    server.value = { ...next };
  }
  function updateNetwork(next: NetworkConfig) {
    network.value = { ...next };
  }

  function updatePaths(next: AppPaths) {
    paths.value = { ...next };
  }

  function setDefaultVersion(v: string) {
    defaultVersion.value = v;
  }

  /** 某个版本被多少项目使用 —— 设置页里用来防止误删 */
  function versionUsage(v: string) {
    return projects.value.filter((p) => p.version === v).length;
  }

  // ───────────────────────── 真实版本下载 ─────────────────────────
  async function installVersion(v: string): Promise<{ ok: boolean; reason?: string }> {
    if (!inTauri()) {
      const it = versionList.value.find((x) => x.version === v);
      if (it) it.installed = true;
      return { ok: true };
    }
    installProgress.value[v] = { phase: "connect", pct: 0 };
    try {
      await invoke<string>("version_install", {
        version: v,
        proxy: network.value.useProxy ? network.value.proxyUrl : null,
        mirror: network.value.mirror?.trim() || null,
      });
      await refreshLocalVersions();
      installProgress.value[v] = { phase: "done", pct: 100 };
      return { ok: true };
    } catch (e) {
      delete installProgress.value[v];
      return { ok: false, reason: String(e) };
    }
  }

  async function removeVersion(v: string): Promise<{ ok: boolean; reason?: string }> {
    if (v === defaultVersion.value) return { ok: false, reason: "该版本是全局默认版本，先换一个默认版本再删除" };
    const used = versionUsage(v);
    if (used > 0) return { ok: false, reason: `还有 ${used} 个项目在使用 ${v}，请先在配置页改掉这些项目的版本` };
    const it = versionList.value.find((x) => x.version === v);
    if (!it || !it.installed) return { ok: false, reason: "该版本尚未安装" };
    if (inTauri()) {
      try {
        await invoke("version_remove", { version: v });
      } catch (e) {
        return { ok: false, reason: String(e) };
      }
    }
    it.installed = false;
    it.size = undefined;
    return { ok: true };
  }

  // ───────────────────────── 真实进程 / 服务 ─────────────────────────
  async function projectTomlText(p: Project) {
    return buildToml(p, server.value, globalTransport.value);
  }

  async function startProject(id: string): Promise<void> {
    const p = projects.value.find((x) => x.id === id);
    if (!p) return;
    if (p.serviceInstalled) {
      throw new Error("该项目已安装为 Windows 服务，由服务托管运行；要改成手动启动请先「卸载服务」。");
    }
    p.state = "starting";
    try {
      const toml = await projectTomlText(p);
      const info = await invoke<ProcInfo>("proc_start", {
        id,
        name: p.name,
        version: p.version,
        toml,
      });
      p.state = "running";
      p.pid = info.pid ?? undefined;
      p.uptime = "刚刚";
      p.updatedAt = nowText();
      if (info.pid) procPid.value[id] = info.pid;
    } catch (e) {
      p.state = "error";
      throw e;
    }
  }

  async function stopProject(id: string): Promise<void> {
    const p = projects.value.find((x) => x.id === id);
    if (!p) return;
    if (inTauri()) {
      await invoke("proc_stop", { id }).catch(() => undefined);
    }
    p.state = "stopped";
    p.pid = undefined;
    p.uptime = undefined;
    tunnelOk.value[id] = false;
    delete procPid.value[id];
  }

  async function restartProject(id: string): Promise<void> {
    await stopProject(id);
    await startProject(id);
  }

  /** 安装为 Windows 服务：先把 toml 落盘，再交给 SCM */
  async function installService(id: string): Promise<void> {
    const p = projects.value.find((x) => x.id === id);
    if (!p) return;
    const toml = await projectTomlText(p);
    if (inTauri()) {
      await invoke("project_write_toml", { id, toml });
      await invoke("service_install", {
        name: p.serviceName,
        display: p.serviceName,
        description: `FRP Tunnel · ${p.name}`,
        id,
        version: p.version,
      });
    }
    p.serviceInstalled = true;
    p.state = "running";
    p.updatedAt = nowText();
  }

  async function uninstallService(id: string): Promise<void> {
    const p = projects.value.find((x) => x.id === id);
    if (!p) return;
    if (inTauri()) {
      await invoke("service_uninstall", { name: p.serviceName });
    }
    p.serviceInstalled = false;
    p.state = "stopped";
    p.pid = undefined;
    p.uptime = undefined;
  }

  /** 轮询：进程是否还活着 + 隧道是否真的连上（frpc /api/status） */
  async function pollOnce() {
    if (!inTauri()) return;
    for (const p of projects.value) {
      if (p.serviceInstalled) continue;
      if (p.state !== "running" && p.state !== "starting") continue;
      try {
        const info = await invoke<ProcInfo>("proc_info", { id: p.id });
        if (!info.running) {
          p.state = "stopped";
          p.pid = undefined;
          p.uptime = undefined;
          tunnelOk.value[p.id] = false;
          continue;
        }
        p.pid = info.pid ?? p.pid;
        p.uptime = `${Math.max(1, Math.floor(info.uptime / 60))} 分钟`;
        const port = adminPort(p.id);
        try {
          const raw = await invoke<string>("frpc_status", { port });
          tunnelOk.value[p.id] = /"status"\s*:\s*"running"/.test(raw);
          if (!tunnelOk.value[p.id] && /"status"\s*:\s*"error"/.test(raw)) p.state = "error";
        } catch {
          tunnelOk.value[p.id] = false;
        }
      } catch {
        /* 单次探测失败不影响整体 */
      }
    }
    if (projects.value.some((p) => p.state === "running" || p.state === "starting")) {
      p_updateNow();
    }
  }

  function p_updateNow() {
    /* 占位：轮询本身不写盘，避免频繁 persist */
  }

  function startPolling() {
    if (!inTauri() || pollTimer) return;
    void pollOnce();
    pollTimer = setInterval(() => void pollOnce(), 2500);
  }

  function stopPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = undefined;
  }

  async function loadProjectLogs(id: string) {
    if (!inTauri()) return;
    try {
      const lines = await invoke<string[]>("proc_logs", { id });
      if (lines.length) projectLogs.value[id] = lines.slice(-800);
    } catch {
      /* ignore */
    }
  }

  return {
    projects,
    selectedId,
    selected,
    filter,
    query,
    filtered,
    counts,
    logs,
    server,
    network,
    globalTransport,
    updateTransport,
    load,
    persist,
    paths,
    versionList,
    versions,
    defaultVersion,
    releaseLoading,
    releaseError,
    fetchReleases,
    theme,
    logAutoScroll,
    select,
    setFilter,
    setState,
    cycleTheme,
    toggleLogAutoScroll,
    setTheme,
    updateServer,
    updateNetwork,
    updatePaths,
    setDefaultVersion,
    versionUsage,
    installVersion,
    removeVersion,
    refreshLocalVersions,
    projectLogs,
    tunnelOk,
    isAdmin,
    checkAdmin,
    relaunchElevated,
    procPid,
    installProgress,
    startProject,
    stopProject,
    restartProject,
    installService,
    uninstallService,
    startPolling,
    stopPolling,
    pollOnce,
    loadProjectLogs,
  };
});
