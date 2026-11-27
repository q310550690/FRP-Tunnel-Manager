import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import type {
  AppPaths,
  FrpVersion,
  ImportedFrpc,
  Project,
  ProxyConfig,
  ServerConfig,
  ServiceState,
  TransportConfig,
} from "@/types";
import { templateByKey } from "@/types";
import { PROJECT_COLORS, TEMPLATE_ICONS } from "@/icons";
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

/** 外部扫描到的 frpc：系统里已装、但不在本程序 versions/ 里的客户端 */
export interface ExternalFrp {
  version: string;
  path: string;
  /** 来源说明，例如「PATH」「C:\Program Files\frp」 */
  source: string;
}

/** 已下载、等待安装的更新包；落盘后下次启动可以直接安装，不必重新检查 */
export interface PendingUpdate {
  version: string;
  path: string;
  assetName: string;
  size: number;
}

/** 打开设置页时要定位并高亮的分区 */
export type SettingsFocus = "server" | "frp" | null;

/** 可导入/导出的配置数据（不含运行时状态） */
export interface ConfigData {
  projects: Project[];
  selectedId: string;
  server: ServerConfig;
  transport: TransportConfig;
  network: NetworkConfig;
  paths: AppPaths;
  defaultVersion: string;
  theme: ThemeMode;
}

export interface ConfigParseResult {
  ok: boolean;
  reason?: string;
  data?: ConfigData;
  count?: number;
  names?: string[];
}

export interface ConfigApplyResult {
  ok: boolean;
  reason?: string;
  count?: number;
}

/** 平台能力：后端 `platform_info` 命令的返回，前端据此决定服务相关文案 */
export interface PlatformInfo {
  os: string;
  /** 「Windows 服务」/「LaunchAgent」/「systemd 用户服务」 */
  serviceKind: string;
  /** 服务标识字段名：「服务名」/「LaunchAgent」/「systemd 单元」 */
  serviceIdLabel: string;
  /** 服务标识前缀（macOS / Linux 为 com.sxq.frptm.，Windows 为空） */
  labelPrefix: string;
  /** 「系统启动时自动拉起」/「登录时自动拉起」 */
  autostart: string;
  needsAdmin: boolean;
  configPath: string;
}

/** 后端返回前的占位值：按 UA 猜一个，避免首屏文案闪成另一个平台 */
function defaultPlatform(): PlatformInfo {
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  if (/Linux/.test(ua) && !/Android/.test(ua)) {
    return {
      os: "linux",
      serviceKind: "systemd 用户服务",
      serviceIdLabel: "systemd 单元",
      labelPrefix: "com.sxq.frptm.",
      autostart: "登录时自动拉起",
      needsAdmin: false,
      configPath: "~/.config/systemd/user/",
    };
  }
  if (/Macintosh|Mac OS X/.test(ua)) {
    return {
      os: "macos",
      serviceKind: "LaunchAgent",
      serviceIdLabel: "LaunchAgent",
      labelPrefix: "com.sxq.frptm.",
      autostart: "登录时自动拉起",
      needsAdmin: false,
      configPath: "~/Library/LaunchAgents/",
    };
  }
  return {
    os: "windows",
    serviceKind: "Windows 服务",
    serviceIdLabel: "服务名",
    labelPrefix: "",
    autostart: "系统启动时自动拉起",
    needsAdmin: true,
    configPath: "",
  };
}

interface VersionProgressPayload {
  version: string;
  phase: string;
  downloaded: number;
  total: number;
}

export const useAppStore = defineStore("app", () => {
  // 首次启动就是一张白纸：不塞任何演示项目 / 演示服务器，等用户自己配置
  const projects = ref<Project[]>([]);
  const selectedId = ref<string>("");
  const filter = ref<FilterKey>("all");
  const query = ref("");
  const theme = ref<ThemeMode>("system");
  const logAutoScroll = ref(true);
  const server = ref<ServerConfig>({
    addr: "",
    port: 7000,
    protocol: "tcp",
    tls: true,
    authMethod: "token",
    token: "",
  });
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
  const versionList = ref<FrpVersion[]>([]);
  const defaultVersion = ref("");

  /** 外部扫描到的 frpc（系统里已安装、尚未纳入本程序版本库） */
  const externalVersions = ref<ExternalFrp[]>([]);
  const externalScanning = ref(false);
  const externalScanned = ref(false);

  /** 已下载、等待安装的更新包 */
  const pendingUpdate = ref<PendingUpdate | null>(null);
  /** store.json 是否已经读取完成（状态栏据此决定何时检查更新） */
  const loaded = ref(false);

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
    /** 是否看过/跳过新手引导 */
    onboarded: boolean;
    /** 已下载、等待安装的更新包 */
    pendingUpdate: PendingUpdate | null;
  }

  /** 是否已经完成（或跳过）新手引导 */
  const onboarded = ref(false);
  /** 新手引导遮罩是否正在展示 */
  const guideOpen = ref(false);
  /** 设置弹窗由 store 统一控制，便于任何页面「去安装 / 去配置」 */
  const settingsOpen = ref(false);
  const settingsFocus = ref<SettingsFocus>(null);

  function snapshot(): PersistShape {
    return {
      version: 3,
      projects: projects.value,
      selectedId: selectedId.value,
      server: server.value,
      transport: globalTransport.value,
      network: network.value,
      paths: paths.value,
      defaultVersion: defaultVersion.value,
      theme: theme.value,
      onboarded: onboarded.value,
      pendingUpdate: pendingUpdate.value,
    };
  }

  // ───────────────────────── 运行时状态（不落盘） ─────────────────────────
  const projectLogs = ref<Record<string, string[]>>({});
  const tunnelOk = ref<Record<string, boolean>>({});

  const isAdmin = ref(false);
  /** 平台能力（服务形态、自启语义…），由 Rust 侧 platform_info 提供 */
  const platformInfo = ref<PlatformInfo>(defaultPlatform());

  async function checkAdmin() {
    if (!inTauri()) return;
    try {
      isAdmin.value = await invoke<boolean>("is_elevated");
    } catch {
      isAdmin.value = false;
    }
  }

  async function loadPlatform() {
    if (!inTauri()) return;
    try {
      platformInfo.value = await invoke<PlatformInfo>("platform_info");
    } catch (e) {
      console.warn("读取平台信息失败：", e);
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

  /** 从 <exe>/config/store.json 读取上次的状态；没有文件就保持空白 */
  async function load() {
    if (!inTauri()) {
      guideOpen.value = !onboarded.value;
      loaded.value = true;
      return;
    }
    try {
      const text = await invoke<string>("read_store");
      if (text && text.trim()) {
        const data = JSON.parse(text) as Partial<PersistShape>;
        const legacy = (data.version ?? 1) < 2;
        let list: Project[] = Array.isArray(data.projects) ? data.projects : [];
        // v1 的演示项目会随存档落盘，升级时按「id + 名称」精确剔除，避免误删用户自己的项目
        if (legacy) {
          const DEMO: Record<string, string> = {
            "rdp-home": "RDP 家用机",
            "vite-blog": "博客前端",
            "home-nas": "家用 NAS",
            "db-mysql": "测试库",
          };
          list = list.filter((p) => !(DEMO[p.id] && DEMO[p.id] === p.name));
        }
        if (list.length) {
          projects.value = list.map((p) => {
            const raw = p as unknown as { state: string; serviceInstalled?: boolean };
            if (raw.state === "not-installed") {
              return { ...p, state: "stopped" as Project["state"], serviceInstalled: false };
            }
            return { ...p, serviceInstalled: raw.serviceInstalled === true };
          });
          selectedId.value =
            data.selectedId && projects.value.some((p) => p.id === data.selectedId)
              ? data.selectedId
              : projects.value[0].id;
        }
        if (data.server) {
          const next = { ...data.server };
          // 203.0.113.0/24 是文档示例网段，不能当成用户真实服务器保留
          if (legacy && next.addr === "203.0.113.10") next.addr = "";
          server.value = { ...server.value, ...next };
        }
        if (data.transport) globalTransport.value = { ...globalTransport.value, ...data.transport };
        if (data.network) network.value = { ...network.value, ...data.network };
        if (data.paths) paths.value = { ...paths.value, ...data.paths };
        if (data.defaultVersion) defaultVersion.value = data.defaultVersion;
        if (data.theme) theme.value = data.theme;
        if (data.pendingUpdate) {
          const u = data.pendingUpdate as Partial<PendingUpdate>;
          if (
            typeof u.version === "string" &&
            u.version &&
            typeof u.path === "string" &&
            u.path &&
            typeof u.assetName === "string" &&
            u.assetName &&
            typeof u.size === "number"
          ) {
            pendingUpdate.value = {
              version: u.version,
              path: u.path,
              assetName: u.assetName,
              size: u.size,
            };
          }
        }
        // v1 存档可能只留下演示项目：清掉后如果什么都没剩，就让新手引导重新出现
        onboarded.value = data.onboarded === true || (legacy && list.length > 0);
      }
    } catch (e) {
      console.warn("读取本地数据失败：", e);
    }
    loaded.value = true;
    guideOpen.value = !onboarded.value;
    bindEvents();
    await refreshLocalVersions();
    void checkAdmin();
    void loadPlatform();
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

  watch(
    [projects, server, globalTransport, network, paths, defaultVersion, theme, onboarded, pendingUpdate],
    persist,
    { deep: true },
  );

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

  function isVersionInstalled(v?: string | null): boolean {
    return !!v && versionList.value.some((x) => x.version === v && x.installed);
  }

  /** 默认版本只做有效性校验：默认版本被删/不存在时清空，不自动改选其他版本 */
  function ensureDefaultVersion() {
    if (defaultVersion.value && !isVersionInstalled(defaultVersion.value)) {
      defaultVersion.value = "";
    }
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
      ensureDefaultVersion();
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
    ensureDefaultVersion();
    releaseLoading.value = false;
  }

  const versions = computed(() =>
    versionList.value.filter((v) => v.installed).map((v) => v.version)
  );
  const hasInstalledFrp = computed(() => versionList.value.some((v) => v.installed));

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
    if (!id || !inTauri() || projectLogs.value[id]?.length) return;
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

  // ───────────────────────── 引导 / 设置入口 ─────────────────────────
  function openGuide() {
    guideOpen.value = true;
  }
  function dismissGuide() {
    guideOpen.value = false;
    onboarded.value = true;
  }
  function markOnboarded() {
    onboarded.value = true;
    guideOpen.value = false;
  }
  function openSettings(focus: SettingsFocus = null) {
    settingsFocus.value = focus;
    settingsOpen.value = true;
    void scanExternal();
  }
  function closeSettings() {
    settingsOpen.value = false;
    settingsFocus.value = null;
  }

  function setPendingUpdate(next: PendingUpdate | null) {
    pendingUpdate.value = next;
  }

  function clearPendingUpdate() {
    pendingUpdate.value = null;
  }

  // ───────────────────────── 外部 FRP 扫描 ─────────────────────────
  async function scanExternal(): Promise<void> {
    if (!inTauri() || externalScanning.value) return;
    externalScanning.value = true;
    try {
      externalVersions.value = await invoke<ExternalFrp[]>("version_scan_external");
    } catch (e) {
      console.warn("扫描外部 frp 失败：", e);
      externalVersions.value = [];
    } finally {
      externalScanning.value = false;
      externalScanned.value = true;
    }
  }

  /** 把外部 frpc 复制进本程序版本库，之后就能和内置版本一样启动/装服务 */
  async function takeOverExternal(item: ExternalFrp): Promise<{ ok: boolean; reason?: string }> {
    if (!inTauri()) return { ok: false, reason: "浏览器预览无法接管本机文件" };
    try {
      await invoke<string>("version_take_over", { version: item.version, path: item.path });
      await refreshLocalVersions();
      ensureDefaultVersion();
      await scanExternal();
      return { ok: true };
    } catch (e) {
      return { ok: false, reason: String(e) };
    }
  }

  /** 手动选择一个 frpc 文件：只探测版本，不复制；用户确认后再走 takeOverExternal */
  async function probeExternalFile(
    path: string,
  ): Promise<{ ok: boolean; item?: ExternalFrp; reason?: string }> {
    if (!inTauri()) return { ok: false, reason: "浏览器预览无法读取本机文件" };
    try {
      const item = await invoke<ExternalFrp>("version_probe_file", { path });
      return { ok: true, item };
    } catch (e) {
      return { ok: false, reason: String(e) };
    }
  }

  // ───────────────────────── 真实版本下载 ─────────────────────────
  async function installVersion(v: string): Promise<{ ok: boolean; reason?: string }> {
    if (!inTauri()) {
      const it = versionList.value.find((x) => x.version === v);
      if (it) it.installed = true;
      ensureDefaultVersion();
      if (!defaultVersion.value) defaultVersion.value = v;
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
      if (!defaultVersion.value) defaultVersion.value = v;
      installProgress.value[v] = { phase: "done", pct: 100 };
      return { ok: true };
    } catch (e) {
      delete installProgress.value[v];
      return { ok: false, reason: String(e) };
    }
  }

  async function removeVersion(v: string): Promise<{ ok: boolean; reason?: string }> {
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
    // 默认版本被删掉后不自动挑别的版本：保留「无默认版本」状态即可
    if (defaultVersion.value === v) defaultVersion.value = "";
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
      throw new Error(
        `该项目已安装为${platformInfo.value.serviceKind}，由服务托管运行；要改成手动启动请先「卸载服务」。`,
      );
    }
    if (!p.version) p.version = defaultVersion.value || versions.value[0] || "";
    if (!isVersionInstalled(p.version)) {
      throw new Error(
        p.version
          ? `frp ${p.version} 还没安装，请先到「设置 → FRP 版本」里安装一个`
          : "检测到 FRP 未安装，请先安装一个",
      );
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

  /** 安装为服务：Windows 注册 SCM 服务，macOS 写 launchd LaunchAgent */
  async function installService(id: string): Promise<void> {
    const p = projects.value.find((x) => x.id === id);
    if (!p) return;
    if (!p.version) p.version = defaultVersion.value || versions.value[0] || "";
    if (!isVersionInstalled(p.version)) {
      throw new Error(
        p.version
          ? `frp ${p.version} 还没安装，请先到「设置 → FRP 版本」里安装一个`
          : "检测到 FRP 未安装，请先安装一个",
      );
    }
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
      await invoke("service_uninstall", { name: p.serviceName, id: p.id });
    }
    p.serviceInstalled = false;
    p.state = "stopped";
    p.pid = undefined;
    p.uptime = undefined;
  }

  /** 删除项目：先停进程，再删 projects/<id> 整目录（frpc.toml + 日志） */
  async function deleteProject(id: string): Promise<void> {
    const idx = projects.value.findIndex((x) => x.id === id);
    if (idx < 0) return;
    const p = projects.value[idx];
    if (p.serviceInstalled) {
      throw new Error("该项目已安装为服务，请先卸载服务再删除。");
    }
    if (p.state === "running" || p.state === "starting") {
      await stopProject(id);
    }
    if (inTauri()) {
      await invoke("project_delete", { id });
    }
    projects.value.splice(idx, 1);
    delete projectLogs.value[id];
    delete tunnelOk.value[id];
    delete procPid.value[id];
    if (selectedId.value === id) {
      const next = projects.value[Math.min(idx, projects.value.length - 1)];
      selectedId.value = next?.id ?? "";
      if (next) void ensureLogs(next.id);
    }
  }

  function sameServer(a: ServerConfig, b: ServerConfig): boolean {
    return (
      a.addr === b.addr &&
      a.port === b.port &&
      a.protocol === b.protocol &&
      a.tls === b.tls &&
      a.authMethod === b.authMethod &&
      a.token === b.token
    );
  }

  function sameTransport(a: TransportConfig, b: TransportConfig): boolean {
    return (
      a.tcpMux === b.tcpMux &&
      a.poolCount === b.poolCount &&
      a.heartbeatInterval === b.heartbeatInterval
    );
  }

  /** 导入用户已有的 frpc.toml；一条代理生成一个项目，文件本身不改动 */
  async function importFrpcToml(
    path: string,
  ): Promise<{ ok: boolean; reason?: string; count?: number; names?: string[]; warnings?: string[] }> {
    if (!inTauri()) return { ok: false, reason: "浏览器预览无法读取本机 frpc.toml" };
    let cfg: ImportedFrpc;
    try {
      cfg = await invoke<ImportedFrpc>("project_import_toml", { path });
    } catch (e) {
      return { ok: false, reason: String(e) };
    }
    if (!cfg.proxies.length) return { ok: false, reason: "frpc.toml 里没有可导入的代理" };

    const cfgServer: ServerConfig = {
      addr: cfg.serverAddr,
      port: cfg.serverPort,
      protocol: cfg.protocol,
      tls: cfg.tls,
      authMethod: cfg.authMethod,
      token: cfg.token,
    };
    const cfgTransport: TransportConfig = {
      tcpMux: cfg.tcpMux,
      poolCount: cfg.poolCount,
      heartbeatInterval: cfg.heartbeatInterval,
    };
    const ownServer = sameServer(cfgServer, server.value) ? null : cfgServer;
    const ownTransport = sameTransport(cfgTransport, globalTransport.value) ? null : cfgTransport;
    const version = defaultVersion.value || versions.value[0] || "";
    const added: Project[] = [];
    const warnings: string[] = [];

    // 导入的项目统一先落到「空白项目」，之后可以在配置页重新选择类型
    const tpl = templateByKey("blank");
    if (!tpl) return { ok: false, reason: "缺少「空白项目」模板" };

    for (const src of cfg.proxies) {
      const templateKey = "blank";
      const name = src.name.trim() || `导入项目 ${added.length + 1}`;
      const proxy: ProxyConfig = {
        name,
        type: src.type,
        localIP: src.localIP || "127.0.0.1",
        localPort: src.localPort,
      };
      if ((src.type === "tcp" || src.type === "udp") && src.remotePort) {
        proxy.remotePort = src.remotePort;
      }
      if (src.customDomains.length) {
        proxy.customDomain = src.customDomains[0];
        if (src.customDomains.length > 1) {
          warnings.push(`「${name}」有 ${src.customDomains.length} 个域名，只保留了 ${src.customDomains[0]}`);
        }
      }
      if (src.subdomain) proxy.subdomain = src.subdomain;
      if (src.hostHeader) proxy.hostHeader = src.hostHeader;
      if (src.httpUser) proxy.httpUser = src.httpUser;
      if (src.httpPassword) proxy.httpPassword = src.httpPassword;
      if (src.secretKey) proxy.secretKey = src.secretKey;

      const item: Project = {
        id: `${templateKey}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`,
        name,
        template: tpl.name,
        templateKey: tpl.key,
        icon: TEMPLATE_ICONS[tpl.key] ?? "file",
        color: PROJECT_COLORS[(projects.value.length + added.length) % PROJECT_COLORS.length],
        proxy,
        serviceName: `FRP Tunnel · ${name}`,
        version,
        state: "stopped",
        updatedAt: "刚刚",
        server: ownServer ? { ...ownServer } : null,
        transport: ownTransport ? { ...ownTransport } : null,
      };
      projects.value.push(item);
      added.push(item);
    }

    if (!added.length) return { ok: false, reason: "没有导入任何项目" };
    selectedId.value = added[0].id;
    markOnboarded();
    return {
      ok: true,
      count: added.length,
      names: added.map((p) => p.name),
      warnings,
    };
  }

  // ───────────────────────── 配置导入 / 导出 ─────────────────────────
  const CONFIG_SCHEMA = "frp-tunnel-manager/config";
  const CONFIG_VERSION = 1;
  const CONFIG_PROTOCOLS = ["kcp", "tcp", "quic", "ws"] as const;
  const CONFIG_PROXY_TYPES = ["tcp", "udp", "http", "https", "stcp", "xtcp", "sudp"] as const;
  const CONFIG_AUTH_METHODS = ["token", "none"] as const;
  const CONFIG_THEMES = ["system", "dark", "light"] as const;

  function isRecord(v: unknown): v is Record<string, unknown> {
    return typeof v === "object" && v !== null && !Array.isArray(v);
  }

  function readText(v: unknown, fallback = ""): string {
    return typeof v === "string" ? v : fallback;
  }

  function readInt(v: unknown, fallback: number, min: number, max: number): number {
    const n = typeof v === "number" ? v : Number(v);
    if (!Number.isFinite(n)) return fallback;
    return Math.min(max, Math.max(min, Math.trunc(n)));
  }

  function readBool(v: unknown, fallback: boolean): boolean {
    return typeof v === "boolean" ? v : fallback;
  }

  function readOneOf<T extends string>(v: unknown, allowed: readonly T[], fallback: T): T {
    return typeof v === "string" && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  }

  function readServer(v: unknown, base: ServerConfig): ServerConfig {
    const r = isRecord(v) ? v : {};
    return {
      addr: readText(r.addr, base.addr).trim(),
      port: readInt(r.port, base.port, 1, 65535),
      protocol: readOneOf(r.protocol, CONFIG_PROTOCOLS, base.protocol),
      tls: readBool(r.tls, base.tls),
      authMethod: readOneOf(r.authMethod, CONFIG_AUTH_METHODS, base.authMethod),
      token: readText(r.token, base.token),
    };
  }

  function readTransport(v: unknown, base: TransportConfig): TransportConfig {
    const r = isRecord(v) ? v : {};
    return {
      tcpMux: readBool(r.tcpMux, base.tcpMux),
      poolCount: readInt(r.poolCount, base.poolCount, 0, 1024),
      heartbeatInterval: readInt(r.heartbeatInterval, base.heartbeatInterval, 0, 3600),
    };
  }

  function readProxy(v: unknown, fallbackName: string): Project["proxy"] | string {
    const r = isRecord(v) ? v : {};
    const localPort = Number(r.localPort);
    if (!Number.isInteger(localPort) || localPort < 1 || localPort > 65535) {
      return "本地端口必须是 1–65535 的整数";
    }
    const proxy: Project["proxy"] = {
      name: readText(r.name, fallbackName).trim() || fallbackName,
      type: readOneOf(r.type, CONFIG_PROXY_TYPES, "tcp"),
      localIP: readText(r.localIP, "127.0.0.1").trim() || "127.0.0.1",
      localPort,
    };
    const remotePort = Number(r.remotePort);
    if (Number.isInteger(remotePort) && remotePort >= 1 && remotePort <= 65535) {
      proxy.remotePort = remotePort;
    }
    for (const key of ["customDomain", "subdomain", "hostHeader", "httpUser", "httpPassword", "secretKey"] as const) {
      const val = readText(r[key]).trim();
      if (val) proxy[key] = val;
    }
    return proxy;
  }

  function readProject(v: unknown, index: number): Project | string {
    const where = `第 ${index + 1} 个项目`;
    if (!isRecord(v)) return `${where}格式不正确`;
    const id = readText(v.id).trim();
    if (!/^[A-Za-z0-9_-]{1,64}$/.test(id) || ["__proto__", "prototype", "constructor"].includes(id)) {
      return `${where}的 id 不合法`;
    }
    const name = readText(v.name).trim();
    if (!name) return `${where}缺少名称`;
    const proxy = readProxy(v.proxy, name);
    if (typeof proxy === "string") return `${where}：${proxy}`;
    const project: Project = {
      id,
      name,
      template: readText(v.template, "自定义").trim() || "自定义",
      templateKey: readText(v.templateKey, "blank").trim() || "blank",
      icon: readText(v.icon, "file").trim() || "file",
      proxy,
      serviceName: readText(v.serviceName, `FRP Tunnel · ${name}`).trim() || `FRP Tunnel · ${name}`,
      version: readText(v.version).trim(),
      state: "stopped",
      updatedAt: readText(v.updatedAt, nowText()),
    };
    const color = readText(v.color).trim();
    if (color) project.color = color;
    if (v.server !== undefined && v.server !== null) {
      if (!isRecord(v.server)) return `${where}的服务器配置格式不正确`;
      project.server = readServer(v.server, server.value);
    } else {
      project.server = null;
    }
    if (v.transport !== undefined && v.transport !== null) {
      if (!isRecord(v.transport)) return `${where}的传输配置格式不正确`;
      project.transport = readTransport(v.transport, globalTransport.value);
    } else {
      project.transport = null;
    }
    if (v.optim !== undefined && v.optim !== null) {
      if (!isRecord(v.optim)) return `${where}的优化项格式不正确`;
      const optim: Record<string, boolean | number> = {};
      for (const [key, val] of Object.entries(v.optim)) {
        if (typeof val === "boolean") optim[key] = val;
        else if (typeof val === "number" && Number.isFinite(val)) optim[key] = val;
      }
      project.optim = optim;
    }
    return project;
  }

  /** 解析配置文件；只保留受支持的字段，并把运行状态重置为停止 */
  function parseConfigText(text: string): ConfigParseResult {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      return { ok: false, reason: "不是有效的 JSON 文件" };
    }
    if (!isRecord(parsed)) return { ok: false, reason: "配置文件内容不是一个对象" };
    if (parsed.schema !== undefined && parsed.schema !== CONFIG_SCHEMA) {
      return { ok: false, reason: "不是 FRP Tunnel Manager 的配置文件" };
    }
    if (parsed.schema === CONFIG_SCHEMA && parsed.version !== CONFIG_VERSION) {
      return { ok: false, reason: `不支持的配置版本：${String(parsed.version)}` };
    }
    const payload = isRecord(parsed.data) ? parsed.data : parsed;
    if (!Array.isArray(payload.projects)) return { ok: false, reason: "配置文件里缺少项目列表" };
    if (payload.projects.length > 200) return { ok: false, reason: "项目数量超过 200，已取消导入" };

    const list: Project[] = [];
    const ids = new Set<string>();
    for (let i = 0; i < payload.projects.length; i++) {
      const item = readProject(payload.projects[i], i);
      if (typeof item === "string") return { ok: false, reason: item };
      if (ids.has(item.id)) return { ok: false, reason: `项目 id 重复：${item.id}` };
      ids.add(item.id);
      list.push(item);
    }

    const rawPaths = isRecord(payload.paths) ? payload.paths : {};
    const rawNetwork = isRecord(payload.network) ? payload.network : {};
    const selected = readText(payload.selectedId).trim();
    return {
      ok: true,
      count: list.length,
      names: list.slice(0, 5).map((p) => p.name),
      data: {
        projects: list,
        selectedId: ids.has(selected) ? selected : (list[0]?.id ?? ""),
        server: readServer(payload.server, server.value),
        transport: readTransport(payload.transport, globalTransport.value),
        network: {
          useProxy: readBool(rawNetwork.useProxy, network.value.useProxy),
          proxyUrl: readText(rawNetwork.proxyUrl, network.value.proxyUrl).trim(),
          mirror: readText(rawNetwork.mirror, network.value.mirror).trim(),
        },
        paths: {
          projects: readText(rawPaths.projects, paths.value.projects).trim() || paths.value.projects,
          config: readText(rawPaths.config, paths.value.config).trim() || paths.value.config,
        },
        defaultVersion: readText(payload.defaultVersion, defaultVersion.value).trim(),
        theme: readOneOf(payload.theme, CONFIG_THEMES, theme.value),
      },
    };
  }

  /** 导出当前配置；项目里的运行状态与 pid 等一律不写出去 */
  function exportConfigJson(): string {
    const data: ConfigData = {
      projects: projects.value.map((p) => ({
        ...p,
        state: "stopped",
        pid: undefined,
        uptime: undefined,
        trafficIn: undefined,
        trafficOut: undefined,
        serviceInstalled: false,
      })),
      selectedId: selectedId.value,
      server: { ...server.value },
      transport: { ...globalTransport.value },
      network: { ...network.value },
      paths: { ...paths.value },
      defaultVersion: defaultVersion.value,
      theme: theme.value,
    };
    return JSON.stringify(
      { schema: CONFIG_SCHEMA, version: CONFIG_VERSION, exportedAt: new Date().toISOString(), data },
      null,
      2,
    );
  }

  /** 应用已经解析、校验过的配置；调用方需要先停止正在运行的项目 */
  async function applyConfigData(data: ConfigData): Promise<ConfigApplyResult> {
    if (projects.value.some((p) => p.serviceInstalled)) {
      return { ok: false, reason: "当前有已安装为服务的项目，请先卸载服务再导入配置。" };
    }
    if (projects.value.some((p) => p.state === "running" || p.state === "starting")) {
      return { ok: false, reason: "请先停止正在运行的项目，再导入配置。" };
    }
    projects.value = data.projects.map((p) => ({
      ...p,
      state: "stopped",
      serviceInstalled: false,
      pid: undefined,
      uptime: undefined,
      trafficIn: undefined,
      trafficOut: undefined,
    }));
    selectedId.value = projects.value.some((p) => p.id === data.selectedId)
      ? data.selectedId
      : (projects.value[0]?.id ?? "");
    server.value = { ...data.server };
    globalTransport.value = { ...data.transport };
    network.value = { ...data.network };
    paths.value = { ...data.paths };
    defaultVersion.value = data.defaultVersion;
    theme.value = data.theme;
    projectLogs.value = {};
    tunnelOk.value = {};
    procPid.value = {};
    onboarded.value = true;
    guideOpen.value = false;
    await refreshLocalVersions();
    ensureDefaultVersion();
    if (selectedId.value) void ensureLogs(selectedId.value);
    persist();
    return { ok: true, count: projects.value.length };
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
    platformInfo,
    loadPlatform,
    relaunchElevated,
    procPid,
    installProgress,
    startProject,
    stopProject,
    restartProject,
    installService,
    uninstallService,
    deleteProject,
    importFrpcToml,
    startPolling,
    stopPolling,
    pollOnce,
    loadProjectLogs,
    onboarded,
    guideOpen,
    settingsOpen,
    settingsFocus,
    externalVersions,
    externalScanning,
    externalScanned,
    pendingUpdate,
    loaded,
    setPendingUpdate,
    clearPendingUpdate,
    hasInstalledFrp,
    isVersionInstalled,
    ensureDefaultVersion,
    scanExternal,
    probeExternalFile,
    takeOverExternal,
    openGuide,
    dismissGuide,
    markOnboarded,
    openSettings,
    closeSettings,
    exportConfigJson,
    parseConfigText,
    applyConfigData,
  };
});
