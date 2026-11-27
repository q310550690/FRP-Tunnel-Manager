<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useAppStore } from "@/stores/app";
import { adminPort } from "@/utils/toml";
import {
  TEMPLATES,
  templateByKey,
  type FieldDef,
  type ProxyType,
  type ServerConfig,
  type TransportConfig,
} from "@/types";
import { TEMPLATE_ICONS } from "@/icons";
import { optimsFor, type OptimItem } from "@/optimizations";
import { isTauri, rdpGet, rdpSet } from "@/services/rdp";
import { t } from "@/i18n";
import {
  fieldHint,
  fieldLabel,
  fieldPlaceholder,
  optimHint,
  optimLabel,
  templateName,
} from "@/i18n/labels";
import { ElMessage } from "element-plus";
import AppIcon from "./AppIcon.vue";

const store = useAppStore();
const project = computed(() => store.selected!);

const form = reactive({
  name: "",
  type: "tcp" as ProxyType,
  localIP: "127.0.0.1",
  localPort: 3389,
  remotePort: 0,
  customDomain: "",
  subdomain: "",
  hostHeader: "",
  httpUser: "",
  httpPassword: "",
  secretKey: "",
  version: "0.61.1",
});

/** 项目级服务器覆盖 */
const useOwnServer = ref(false);
const srv = reactive<ServerConfig>({ ...store.server });

/** 项目级传输参数覆盖 */
const useOwnTransport = ref(false);
const transport = reactive<TransportConfig>({ ...store.globalTransport });

const tmpl = computed(() => templateByKey(project.value?.templateKey ?? project.value?.template));

/** 项目类型（模板）可以在配置页随时切换；只改分类和专属字段，不动已有端口/域名 */
const projectTemplateKey = computed({
  get: () =>
    project.value?.templateKey ?? templateByKey(project.value?.template)?.key ?? "",
  set: (key: string) => {
    const p = project.value;
    const tpl = templateByKey(key);
    if (!p || !tpl) return;
    p.templateKey = tpl.key;
    p.template = tpl.name;
    p.icon = TEMPLATE_ICONS[tpl.key] ?? p.icon;
  },
});

/** 公网端口已经并到「映射」里，专属配置只保留其它字段 */
const fields = computed<FieldDef[]>(() => (tmpl.value?.fields ?? []).filter((f) => f.key !== "remotePort"));

const isWeb = computed(() => form.type === "http" || form.type === "https");
const isTcp = computed(() => form.type === "tcp" || form.type === "udp");
const isStcp = computed(() => form.type === "stcp" || form.type === "xtcp");
const showRemote = computed(() => isTcp.value && !isStcp.value);

const optims = computed<OptimItem[]>(() => optimsFor(project.value?.templateKey));
const osOptims = computed(() => optims.value.filter((o) => o.group === "os"));
/** 「系统与策略」写的是 Windows 注册表，macOS 上整组隐藏 */
const osSupported = computed(() => store.platformInfo.os === "windows");

const rdpValues = reactive<Record<string, number>>({});

const effectiveServer = computed<ServerConfig>(() => (useOwnServer.value ? { ...srv } : store.server));
const effectiveTransport = computed<TransportConfig>(() =>
  useOwnTransport.value ? { ...transport } : store.globalTransport,
);
/** 已安装版本 + 项目当前版本（版本被删掉时也不至于显示空白） */
const versionOptions = computed(() => {
  const list = [...store.versions];
  if (form.version && !list.includes(form.version)) list.unshift(form.version);
  return list;
});

async function loadRdp() {
  if (!osSupported.value || !osOptims.value.length) return;
  const live = await rdpGet();
  for (const o of osOptims.value) {
    const v = live[o.key];
    rdpValues[o.key] = typeof v === "number" ? v : 0;
  }
}

async function toggleRdp(o: OptimItem, on: boolean) {
  const want = on ? (o.onValue ?? 1) : null;
  try {
    if (isTauri) await rdpSet(o.key, want);
    rdpValues[o.key] = want ?? 0;
    ElMessage({
      message: on
        ? t("已写入：{name}", { name: optimLabel(o) })
        : t("已恢复「未配置」：{name}", { name: optimLabel(o) }),
      type: "success",
      duration: 1600,
    });
  } catch (e) {
    ElMessage({
      message: t("写入注册表失败：{error}（需要以管理员身份运行）", { error: String(e) }),
      type: "error",
      duration: 4000,
    });
  }
}

function reset() {
  const p = project.value;
  if (!p) return;
  form.name = p.proxy.name;
  form.type = p.proxy.type;
  form.localIP = p.proxy.localIP;
  form.localPort = p.proxy.localPort;
  form.remotePort = p.proxy.remotePort ?? 0;
  form.customDomain = p.proxy.customDomain ?? "";
  form.subdomain = p.proxy.subdomain ?? "";
  form.hostHeader = p.proxy.hostHeader ?? "";
  form.httpUser = p.proxy.httpUser ?? "";
  form.httpPassword = p.proxy.httpPassword ?? "";
  form.secretKey = p.proxy.secretKey ?? "";
  form.version = p.version;
  useOwnServer.value = !!p.server;
  Object.assign(srv, p.server ?? store.server);
  useOwnTransport.value = !!p.transport;
  Object.assign(transport, p.transport ?? store.globalTransport);
}

watch(project, reset, { immediate: true });
watch(optims, () => { void loadRdp(); }, { immediate: true });

const toml = computed(() => {
  const s = effectiveServer.value;
  const auth = s.authMethod === "token" && s.token ? ["", "[auth]", 'method = "token"', `token = "${s.token}"`] : [];
  const tplLabel = templateName(tmpl.value?.key, project.value?.template);
  const lines = [
    `# ${project.value?.name} · ${t("由模板「{name}」生成", { name: tplLabel })}`,
    "",
    `serverAddr = "${s.addr}"`,
    `serverPort = ${s.port}`,
    ...auth,
    "",
    "[transport]",
    `protocol = "${s.protocol}"`,
    `tcpMux = ${effectiveTransport.value.tcpMux}`,
    `poolCount = ${effectiveTransport.value.poolCount}`,
    `heartbeatInterval = ${effectiveTransport.value.heartbeatInterval}`,
    "",
    "[transport.tls]",
    `enable = ${s.tls}`,
    "",
    "[webServer]",
    'addr = "127.0.0.1"',
    `port = ${adminPort(project.value?.id ?? "x")}`,
    "",
    "[[proxies]]",
    `name = "${form.name}"`,
    `type = "${form.type}"`,
    `localIP = "${form.localIP}"`,
    `localPort = ${form.localPort}`,
  ];
  if (showRemote.value && form.remotePort) lines.push(`remotePort = ${form.remotePort}`);
  if (isWeb.value) {
    if (form.customDomain) lines.push(`customDomains = ["${form.customDomain}"]`);
    if (form.subdomain) lines.push(`subdomain = "${form.subdomain}"`);
    if (form.hostHeader) lines.push(`hostHeader = "${form.hostHeader}"`);
    if (form.httpUser) {
      lines.push("[proxies.httpUser]", `user = "${form.httpUser}"`);
      if (form.httpPassword) lines.push(`password = "${form.httpPassword}"`);
    }
  }
  if (isStcp.value && form.secretKey) lines.push(`secretKey = "${form.secretKey}"`);
  return lines.join("\n");
});

function save() {
  const p = project.value;
  if (!p) return;
  p.proxy.name = form.name;
  p.proxy.type = form.type;
  p.proxy.localIP = form.localIP;
  p.proxy.localPort = Number(form.localPort);
  p.proxy.remotePort = form.remotePort ? Number(form.remotePort) : undefined;
  p.proxy.customDomain = form.customDomain || undefined;
  p.proxy.subdomain = form.subdomain || undefined;
  p.proxy.hostHeader = form.hostHeader || undefined;
  p.proxy.httpUser = form.httpUser || undefined;
  p.proxy.httpPassword = form.httpPassword || undefined;
  p.proxy.secretKey = form.secretKey || undefined;
  p.version = form.version;
  p.server = useOwnServer.value ? { ...srv } : null;
  p.transport = useOwnTransport.value ? { ...transport } : null;
  const next: Record<string, boolean | number> = {};
  for (const o of osOptims.value) next[o.key] = rdpValues[o.key] ? true : false;
  p.optim = next;
  ElMessage({ message: t("已保存，重启服务后生效"), type: "success", duration: 2000 });
}
</script>

<template>
  <div class="panel">
    <Teleport to="#cfg-actions" defer>
      <button class="btn sm" @click="reset"><AppIcon name="restart" :size="12" />{{ $t("还原") }}</button>
      <button class="btn sm accent" @click="save"><AppIcon name="ok" :size="12" />{{ $t("保存配置") }}</button>
    </Teleport>

    <div class="section">
      <div class="section-head">
        <span class="label">{{ $t("项目") }}</span>
      </div>
      <div class="rows">
        <div class="row">
          <label>{{ $t("项目名称") }}</label>
          <div class="ctl"><el-input v-model="form.name" size="small" /></div>
        </div>
        <div class="row">
          <label>{{ $t("类型") }}</label>
          <div class="ctl">
            <el-select v-model="projectTemplateKey" size="small" :placeholder="$t('选择项目类型')">
              <el-option v-for="t in TEMPLATES" :key="t.key" :label="templateName(t.key)" :value="t.key" />
            </el-select>
          </div>
        </div>
        <div class="row">
          <label>{{ $t("frp 版本") }}</label>
          <div class="ctl">
            <el-select
              v-model="form.version"
              size="small"
              :placeholder="
                store.defaultVersion
                  ? $t('跟随全局 {version}', { version: store.defaultVersion })
                  : $t('未安装')
              "
            >
              <el-option v-for="v in versionOptions" :key="v" :label="v" :value="v" />
            </el-select>
            <span class="hint">{{ $t("默认跟随全局 {version}，项目内可单独指定", { version: store.defaultVersion || $t("未安装") }) }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">{{ $t("服务器") }}</span>
        <span class="hint mono">
          {{
            useOwnServer
              ? $t("项目独立配置")
              : $t("跟随全局 {addr}:{port}", { addr: store.server.addr, port: store.server.port })
          }}
        </span>
      </div>
      <div class="rows">
        <div class="row">
          <label>{{ $t("独立配置") }}</label>
          <div class="ctl">
            <el-switch v-model="useOwnServer" size="small" />
            <span class="hint">{{ $t("关闭时使用设置页里的全局服务器；打开后可让这个项目连另一台 frps。") }}</span>
          </div>
        </div>
        <template v-if="useOwnServer">
          <div class="row">
            <label>{{ $t("服务器地址") }}</label>
            <div class="ctl"><el-input v-model="srv.addr" size="small" :placeholder="$t('例如 203.0.113.10')" /></div>
          </div>
          <div class="row">
            <label>{{ $t("服务器端口") }}</label>
            <div class="ctl">
              <el-input-number v-model="srv.port" :min="1" :max="65535" size="small" controls-position="right" />
            </div>
          </div>
          <div class="row">
            <label>{{ $t("传输协议") }}</label>
            <div class="ctl">
              <el-select v-model="srv.protocol" size="small">
                <el-option label="KCP" value="kcp" />
                <el-option label="TCP" value="tcp" />
                <el-option label="QUIC" value="quic" />
                <el-option label="WebSocket" value="ws" />
              </el-select>
            </div>
          </div>
          <div class="row">
            <label>TLS</label>
            <div class="ctl">
              <el-switch v-model="srv.tls" size="small" />
              <span class="hint">{{ $t("公网建议开启，需与 frps 的 transport.tls 保持一致。") }}</span>
            </div>
          </div>
          <div class="row">
            <label>{{ $t("认证方式") }}</label>
            <div class="ctl">
              <el-select v-model="srv.authMethod" size="small">
                <el-option label="Token" value="token" />
                <el-option :label="$t('不认证')" value="none" />
              </el-select>
            </div>
          </div>
          <div v-if="srv.authMethod === 'token'" class="row">
            <label>Token</label>
            <div class="ctl">
              <el-input v-model="srv.token" type="password" show-password size="small" />
              <span class="hint">{{ $t("需与 frps 的 auth.token 一致。") }}</span>
            </div>
          </div>
        </template>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">{{ $t("映射") }}</span>
      </div>
      <div class="rows">
        <div class="row">
          <label>{{ $t("代理类型") }}</label>
          <div class="ctl">
            <el-select v-model="form.type" size="small">
              <el-option label="TCP" value="tcp" />
              <el-option label="UDP" value="udp" />
              <el-option label="HTTP" value="http" />
              <el-option label="HTTPS" value="https" />
              <el-option label="STCP" value="stcp" />
            </el-select>
          </div>
        </div>
        <div class="row">
          <label>{{ $t("本地地址") }}</label>
          <div class="ctl">
            <el-input v-model="form.localIP" size="small" />
            <span class="hint">{{ $t("被穿透服务在本机监听的地址，通常是 127.0.0.1") }}</span>
          </div>
        </div>
        <div class="row">
          <label>{{ $t("本地端口") }}</label>
          <div class="ctl">
            <el-input-number v-model="form.localPort" :min="1" :max="65535" size="small" controls-position="right" />
          </div>
        </div>
        <div v-if="showRemote" class="row">
          <label>{{ $t("公网端口") }}</label>
          <div class="ctl">
            <el-input-number v-model="form.remotePort" :min="1" :max="65535" size="small" controls-position="right" />
            <span class="hint">{{ $t("服务器上暴露的端口，外部用「服务器地址:该端口」访问") }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">{{ $t("传输优化") }}</span>
        <span class="hint mono">
          {{
            useOwnTransport
              ? $t("项目独立配置")
              : $t("跟随全局 tcpMux {state} · 池 {pool} · 心跳 {interval}s", {
                  state: store.globalTransport.tcpMux ? $t("开") : $t("关"),
                  pool: store.globalTransport.poolCount,
                  interval: store.globalTransport.heartbeatInterval,
                })
          }}
        </span>
      </div>
      <div class="rows">
        <div class="row">
          <label>{{ $t("独立配置") }}</label>
          <div class="ctl">
            <el-switch v-model="useOwnTransport" size="small" />
            <span class="hint">{{ $t("关闭时使用设置页里的全局传输参数；打开后可单独调整这个项目。") }}</span>
          </div>
        </div>
        <template v-if="useOwnTransport">
          <div v-for="o in optims.filter((x) => x.group === 'frp')" :key="o.key" class="row">
            <label>{{ optimLabel(o) }}</label>
            <div class="ctl">
              <el-switch v-if="o.kind === 'bool'" v-model="(transport as any)[o.key]" size="small" />
              <el-input-number
                v-else
                v-model="(transport as any)[o.key]"
                :min="o.min ?? 0"
                :max="o.max ?? 65535"
                size="small"
                controls-position="right"
              />
              <span class="hint">{{ optimHint(o) }}</span>
            </div>
          </div>
        </template>
      </div>
    </div>

    <div v-if="fields.length || osOptims.length" class="section">
      <div class="section-head">
        <span class="label">{{ $t("专属配置") }}</span>
        <span class="hint mono">{{ templateName(tmpl?.key, tmpl?.name) }} · {{ tmpl?.type }}</span>
      </div>
      <div class="rows">
        <template v-if="fields.length">
          <div v-for="f in fields" :key="f.key" class="row">
            <label>{{ fieldLabel(f) }}</label>
            <div class="ctl">
              <el-input-number
                v-if="f.kind === 'number'"
                v-model="(form as any)[f.key]"
                :min="f.min ?? 1"
                :max="f.max ?? 65535"
                size="small"
                controls-position="right"
              />
              <el-input
                v-else
                v-model="(form as any)[f.key]"
                size="small"
                :type="f.kind === 'password' ? 'password' : 'text'"
                :show-password="f.kind === 'password'"
                :placeholder="fieldPlaceholder(f)"
              />
              <span v-if="f.hint" class="hint">{{ fieldHint(f) }}</span>
            </div>
          </div>
        </template>
        <template v-if="osSupported && osOptims.length">
          <div class="group-label os">{{ $t("系统与策略（写入注册表，需要管理员权限）") }}</div>
          <div v-for="o in osOptims" :key="o.key" class="row">
            <label>{{ optimLabel(o) }}</label>
            <div class="ctl">
              <el-switch :model-value="!!rdpValues[o.key]" size="small" @change="(v: any) => toggleRdp(o, !!v)" />
              <span class="hint">{{ optimHint(o) }}</span>
            </div>
          </div>
        </template>
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <span class="label">{{ $t("生成的 frpc.toml") }}</span>
        <span class="hint mono">./projects/{{ project?.id }}/frpc.toml</span>
      </div>
      <pre class="mono preview-code">{{ toml }}</pre>
    </div>
  </div>
</template>

<style scoped>
.group-label {
  padding: var(--sp-1) var(--sp-2);
  font-size: var(--fs-1);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-faint);
  background: var(--bg);
  border-bottom: 1px solid var(--line);
}
.group-label.os {
  border-top: 1px solid var(--line);
}
.preview-code {
  margin: 0;
  padding: var(--sp-2);
  font-size: var(--fs-2);
  line-height: 1.6;
  color: var(--text-dim);
  overflow-x: auto;
}
</style>
