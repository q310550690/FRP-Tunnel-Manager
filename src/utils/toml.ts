import type { Project, ServerConfig, TransportConfig } from "@/types";

/** 项目没独立配置时跟随全局 */
export function effectiveServer(p: Project, g: ServerConfig): ServerConfig {
  return p.server ?? g;
}

export function effectiveTransport(p: Project, g: TransportConfig): TransportConfig {
  return p.transport ?? g;
}

/** 每个项目分配一个稳定的 frpc 管理端口（127.0.0.1:57400–57599） */
export function adminPort(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return 57400 + (h % 200);
}

/** 由项目当前配置生成 frpc.toml 文本（与配置页预览同一套规则） */
export function buildToml(p: Project, gs: ServerConfig, gt: TransportConfig): string {
  const s = effectiveServer(p, gs);
  const t = effectiveTransport(p, gt);
  const pr = p.proxy;
  const isWeb = pr.type === "http" || pr.type === "https";
  const isTcp = pr.type === "tcp" || pr.type === "udp";
  const auth =
    s.authMethod === "token" && s.token ? ["", "[auth]", 'method = "token"', `token = "${s.token}"`] : [];
  const lines = [
    `# ${p.name} · 由模板「${p.template}」生成`,
    "",
    `serverAddr = "${s.addr}"`,
    `serverPort = ${s.port}`,
    ...auth,
    "",
    "[transport]",
    `protocol = "${s.protocol}"`,
    `tcpMux = ${t.tcpMux}`,
    `poolCount = ${t.poolCount}`,
    `heartbeatInterval = ${t.heartbeatInterval}`,
    "",
    "[transport.tls]",
    `enable = ${s.tls}`,
    "",
    "[webServer]",
    'addr = "127.0.0.1"',
    `port = ${adminPort(p.id)}`,
    "",
    "[[proxies]]",
    `name = "${pr.name}"`,
    `type = "${pr.type}"`,
    `localIP = "${pr.localIP}"`,
    `localPort = ${pr.localPort}`,
  ];
  if (isTcp && pr.remotePort) lines.push(`remotePort = ${pr.remotePort}`);
  if (isWeb) {
    if (pr.customDomain) lines.push(`customDomains = ["${pr.customDomain}"]`);
    if (pr.subdomain) lines.push(`subdomain = "${pr.subdomain}"`);
    if (pr.hostHeader) lines.push(`hostHeader = "${pr.hostHeader}"`);
    if (pr.httpUser) {
      lines.push("[proxies.httpUser]", `user = "${pr.httpUser}"`);
      if (pr.httpPassword) lines.push(`password = "${pr.httpPassword}"`);
    }
  }
  if ((pr.type === "stcp" || pr.type === "xtcp" || pr.type === "sudp") && pr.secretKey) {
    lines.push(`secretKey = "${pr.secretKey}"`);
  }
  return lines.join("\n") + "\n";
}
