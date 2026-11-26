export type ServiceState = "running" | "stopped" | "starting" | "stopping" | "error" | "not-installed";

export type ProxyType = "tcp" | "udp" | "http" | "https" | "stcp" | "xtcp" | "sudp";

export interface ProxyConfig {
  name: string;
  type: ProxyType;
  localIP: string;
  localPort: number;
  /** tcp / udp：服务器上暴露的端口 */
  remotePort?: number;
  /** http / https：自定义域名 */
  customDomain?: string;
  subdomain?: string;
  hostHeader?: string;
  httpUser?: string;
  httpPassword?: string;
  /** stcp / xtcp：访问端配对用的密钥 */
  secretKey?: string;
}

export interface Project {
  id: string;
  name: string;
  /** 模板显示名，例如「远程桌面」 */
  template: string;
  /** 模板 key，例如 rdp —— 决定配置页显示哪些专属字段 */
  templateKey: string;
  icon: string;
  color?: string;
  proxy: ProxyConfig;
  serviceName: string;
  version: string;
  state: ServiceState;
  pid?: number;
  uptime?: string;
  trafficIn?: string;
  trafficOut?: string;
  updatedAt: string;
  /** 项目级服务器覆盖；null/undefined 表示跟随全局设置 */
  server?: ServerConfig | null;
  /** 优化项状态：key → 开关/数值 */
  optim?: Record<string, boolean | number>;
  transport?: TransportConfig | null;
  /** 是否已安装成系统服务（Windows 用 SCM，macOS 用 launchd LaunchAgent，均由服务托管运行） */
  serviceInstalled?: boolean;
}

export type FieldKind = "text" | "number" | "password";

export type FieldKey =
  | "remotePort"
  | "customDomain"
  | "subdomain"
  | "hostHeader"
  | "httpUser"
  | "httpPassword"
  | "secretKey";

/** 模板的专属配置项元数据：配置页据此动态渲染字段 */
export interface FieldDef {
  key: FieldKey;
  label: string;
  kind: FieldKind;
  hint?: string;
  placeholder?: string;
  min?: number;
  max?: number;
}

export interface Template {
  key: string;
  name: string;
  summary: string;
  type: ProxyType;
  defaults: Omit<ProxyConfig, "name">;
  fields: FieldDef[];
}

const F_REMOTE_PORT: FieldDef = {
  key: "remotePort",
  label: "公网端口",
  kind: "number",
  min: 1,
  max: 65535,
  hint: "服务器上暴露的端口，外部通过「服务器地址:该端口」访问",
};

const F_DOMAIN: FieldDef = {
  key: "customDomain",
  label: "自定义域名",
  kind: "text",
  placeholder: "例如 blog.example.com",
  hint: "域名需解析到服务器；服务端要配 vhostHTTPPort / vhostHTTPSPort",
};

const F_SUBDOMAIN: FieldDef = {
  key: "subdomain",
  label: "子域名",
  kind: "text",
  placeholder: "留空则不使用",
  hint: "填写后访问地址为「子域名.服务端 subDomainHost」，与自定义域名二选一",
};

const F_HOST_HEADER: FieldDef = {
  key: "hostHeader",
  label: "Host 头改写",
  kind: "text",
  placeholder: "例如 app.internal",
  hint: "把请求的 Host 头改成后端实际认的域名",
};

const F_HTTP_USER: FieldDef = {
  key: "httpUser",
  label: "HTTP 用户名",
  kind: "text",
  hint: "给这个 Web 代理加一层 Basic Auth，留空则不启用",
};

const F_HTTP_PASS: FieldDef = {
  key: "httpPassword",
  label: "HTTP 密码",
  kind: "password",
  hint: "与用户名成对出现",
};

const F_SECRET_KEY: FieldDef = {
  key: "secretKey",
  label: "预共享密钥",
  kind: "password",
  placeholder: "访问端需填写同一串密钥",
  hint: "stcp 不暴露公网端口，只能由同样持有密钥的访客端连接",
};

export const TEMPLATES: Template[] = [
  {
    key: "rdp",
    name: "远程桌面",
    summary: "Windows RDP，带组策略硬件加速优化",
    type: "tcp",
    defaults: { type: "tcp", localIP: "127.0.0.1", localPort: 3389, remotePort: 6000 },
    fields: [F_REMOTE_PORT],
  },
  {
    key: "vite",
    name: "Vite 前端",
    summary: "本地开发服务器对外暴露，支持自定义域名",
    type: "http",
    defaults: { type: "http", localIP: "127.0.0.1", localPort: 5173, customDomain: "dev.example.com" },
    fields: [F_DOMAIN, F_SUBDOMAIN],
  },
  {
    key: "ssh",
    name: "SSH",
    summary: "命令行远程登录",
    type: "tcp",
    defaults: { type: "tcp", localIP: "127.0.0.1", localPort: 22, remotePort: 6001 },
    fields: [F_REMOTE_PORT],
  },
  {
    key: "web",
    name: "Web 反代",
    summary: "内网 Web 服务穿透，带 Host 头改写与 Basic Auth",
    type: "https",
    defaults: { type: "https", localIP: "127.0.0.1", localPort: 8080, customDomain: "app.example.com" },
    fields: [F_DOMAIN, F_SUBDOMAIN, F_HOST_HEADER, F_HTTP_USER, F_HTTP_PASS],
  },
  {
    key: "db",
    name: "数据库",
    summary: "MySQL / PostgreSQL / Redis 临时访问",
    type: "tcp",
    defaults: { type: "tcp", localIP: "127.0.0.1", localPort: 3306, remotePort: 6010 },
    fields: [F_REMOTE_PORT],
  },
  {
    key: "smb",
    name: "文件共享",
    summary: "SMB / 局域网共享目录",
    type: "tcp",
    defaults: { type: "tcp", localIP: "127.0.0.1", localPort: 445, remotePort: 6011 },
    fields: [F_REMOTE_PORT],
  },
  {
    key: "game",
    name: "游戏联机",
    summary: "UDP 联机端口",
    type: "udp",
    defaults: { type: "udp", localIP: "127.0.0.1", localPort: 25565, remotePort: 6020 },
    fields: [F_REMOTE_PORT],
  },
  {
    key: "stcp",
    name: "点对点（stcp）",
    summary: "不暴露公网端口，需访问端配对",
    type: "stcp",
    defaults: { type: "stcp", localIP: "127.0.0.1", localPort: 3389 },
    fields: [F_SECRET_KEY],
  },
  {
    key: "blank",
    name: "空白项目",
    summary: "从零开始，自己填全部字段",
    type: "tcp",
    defaults: { type: "tcp", localIP: "127.0.0.1", localPort: 8080, remotePort: 6100 },
    fields: [F_REMOTE_PORT, F_DOMAIN],
  },
];

export function templateByKey(key: string | undefined): Template | undefined {
  if (!key) return undefined;
  return TEMPLATES.find((t) => t.key === key) ?? TEMPLATES.find((t) => t.name === key);
}

/** frp 版本仓库里的一条记录 */
export interface FrpVersion {
  version: string;
  installed: boolean;
  size?: string;
  note?: string;
}

/** 客户端认证方式：frp 的 [auth] 段是客户端级配置，所有项目共用 */
export type AuthMethod = "token" | "none";

export interface ServerConfig {
  addr: string;
  port: number;
  protocol: "kcp" | "tcp" | "quic" | "ws";
  tls: boolean;
  authMethod: AuthMethod;
  token: string;
}

/** 全局/项目级传输参数 */
export interface TransportConfig {
  tcpMux: boolean;
  poolCount: number;
  heartbeatInterval: number;
}

/** 便携目录：默认都在程序目录下，不使用 %APPDATA% */
export interface AppPaths {
  projects: string;
  config: string;
}
