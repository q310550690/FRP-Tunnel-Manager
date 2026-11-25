export interface OptimItem {
  key: string;
  label: string;
  hint: string;
  kind: "bool" | "number";
  def: boolean | number;
  /** 勾选时写入的值（仅 os 组的 bool 项，用于把枚举型策略映射到推荐值） */
  onValue?: number;
  min?: number;
  max?: number;
  group: "frp" | "os";
}

/** 传输层优化：写进 frpc.toml 的 [transport] */
const FRP_OPTIMS: OptimItem[] = [
  {
    key: "tcpMux",
    label: "启用 TCP 多路复用",
    hint: "多条代理复用一条连接，减少握手与重连开销。KCP 传输下关闭它会导致登录失败，保持开启。",
    kind: "bool",
    def: true,
    group: "frp",
  },
  {
    key: "poolCount",
    label: "连接池大小",
    hint: "预先与服务端建立 N 条连接，0 表示不启用。适合频繁短连接，长期占用带宽。",
    kind: "number",
    def: 0,
    min: 0,
    max: 50,
    group: "frp",
  },
  {
    key: "heartbeatInterval",
    label: "心跳间隔（秒）",
    hint: "frpc 向服务端发送心跳的间隔。太小浪费流量，太大断线发现慢，默认 30 秒。",
    kind: "number",
    def: 30,
    min: 10,
    max: 600,
    group: "frp",
  },
];

/** 远程桌面专属：Windows 组策略（HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services） */
const RDP_OPTIMS: OptimItem[] = [
  { key: "bEnumerateHWBeforeSW", label: "硬件图形适配器优先", hint: "让会话优先用显卡硬件渲染。需显卡驱动支持 WDDM，否则无效果。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "AVCHardwareEncodePreferred", label: "H.264/AVC 硬件编码", hint: "用显卡编码视频流，明显降低 CPU 占用；出错时自动回退软件编码。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "AVC444ModePreferred", label: "优先 H.264/AVC 444 模式", hint: "444 保留完整色度，文字更锐利但码率更高；带宽紧张时不要开。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "fEnableWddmDriver", label: "使用 WDDM 图形显示驱动", hint: "Win11 默认已启用；老显卡/老驱动才需要关掉回退 XDDM。修改后必须重启。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "MaxCompressionLevel", label: "RemoteFX 数据压缩（平衡）", hint: "勾选写入「平衡内存和带宽」，公网建议开启以改善弱网响应。", kind: "bool", def: false, onValue: 2, group: "os" },
  { key: "GraphicsProfile", label: "RemoteFX 自适应图形", hint: "勾选后由系统按网络状况自动选择体验/带宽策略。", kind: "bool", def: false, onValue: 2, group: "os" },
  { key: "ImageQuality", label: "RemoteFX 图像质量（中）", hint: "勾选写入「中」：牺牲一点清晰度换帧率。", kind: "bool", def: false, onValue: 3, group: "os" },
  { key: "SelectTransport", label: "启用 UDP 传输", hint: "RDP 允许走 UDP，弱网更稳；勾选后为「UDP 或 TCP」。", kind: "bool", def: false, onValue: 2, group: "os" },
  { key: "SelectNetworkDetect", label: "启用网络自动检测", hint: "同时使用连接时间检测与连续网络检测（写入 0）。", kind: "bool", def: false, onValue: 0, group: "os" },
  { key: "fNoRemoteDesktopWallpaper", label: "强制删除远程桌面壁纸", hint: "省一点带宽与解码开销。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "fEnableRemoteFXAdvancedRemoteApp", label: "RemoteApp 高级 RemoteFX", hint: "仅 RemoteApp 场景需要，普通远程桌面不用开。", kind: "bool", def: false, onValue: 1, group: "os" },
  { key: "UseShellAppRuntimeRemoteApp", label: "RemoteApp 增强 shell 体验", hint: "仅 RemoteApp 场景需要（Win11 22H2+）。", kind: "bool", def: false, onValue: 1, group: "os" },
];

/** 按模板返回优化项；远程桌面额外带 Windows 组策略开关 */
export function optimsFor(templateKey: string | undefined): OptimItem[] {
  if (templateKey === "rdp") return [...FRP_OPTIMS, ...RDP_OPTIMS];
  return FRP_OPTIMS;
}
