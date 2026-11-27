//! 解析用户已有的 frpc.toml，转换成前端项目所需的常用字段。
//!
//! 只读取文件，不修改源文件。frp 的高级字段（健康检查、插件、负载均衡等）
//! 不在本程序的配置模型里，导入时会被忽略。

use serde::Serialize;
use std::fs;
use std::path::Path;
use toml::Value;

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PROXIES: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFrpc {
    pub server_addr: String,
    pub server_port: u32,
    pub protocol: String,
    pub tls: bool,
    pub auth_method: String,
    pub token: String,
    pub tcp_mux: bool,
    pub pool_count: u32,
    pub heartbeat_interval: u32,
    pub proxies: Vec<ImportedProxy>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProxy {
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    #[serde(rename = "localIP")]
    pub local_ip: String,
    pub local_port: u32,
    pub remote_port: Option<u32>,
    pub custom_domains: Vec<String>,
    pub subdomain: Option<String>,
    pub host_header: Option<String>,
    pub http_user: Option<String>,
    pub http_password: Option<String>,
    pub secret_key: Option<String>,
}

fn text_at(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn int_at(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_integer)
}

fn bool_at(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

fn parse_frpc_toml(text: &str) -> Result<ImportedFrpc, String> {
    let root: Value = text
        .parse()
        .map_err(|e| format!("解析 frpc.toml 失败：{e}"))?;
    if !root.is_table() {
        return Err("frpc.toml 的内容不是一个 TOML 表".into());
    }

    let server_addr = text_at(&root, "serverAddr")
        .or_else(|| root.get("common").and_then(|c| text_at(c, "server_addr")))
        .ok_or("frpc.toml 里没有找到 serverAddr")?;
    let server_port = int_at(&root, "serverPort")
        .or_else(|| root.get("common").and_then(|c| int_at(c, "server_port")))
        .unwrap_or(7000);
    if !(1..=65535).contains(&server_port) {
        return Err(format!("serverPort 不合法：{server_port}"));
    }

    let transport = root.get("transport");
    let protocol = transport
        .and_then(|t| text_at(t, "protocol"))
        .unwrap_or_else(|| "tcp".into())
        .to_ascii_lowercase();
    let protocol = match protocol.as_str() {
        "tcp" => "tcp",
        "kcp" => "kcp",
        "quic" => "quic",
        "ws" | "websocket" => "ws",
        _ => return Err(format!("不支持的传输协议：{protocol}")),
    }
    .to_string();

    let tls = transport
        .and_then(|t| t.get("tls"))
        .and_then(|t| bool_at(t, "enable"))
        .unwrap_or(true);
    let tcp_mux = transport.and_then(|t| bool_at(t, "tcpMux")).unwrap_or(true);
    let pool_count = transport
        .and_then(|t| int_at(t, "poolCount"))
        .unwrap_or(0)
        .clamp(0, 100) as u32;
    let heartbeat_interval = transport
        .and_then(|t| int_at(t, "heartbeatInterval"))
        .unwrap_or(30)
        .clamp(0, 3600) as u32;

    let auth = root.get("auth");
    let token = auth.and_then(|a| text_at(a, "token")).unwrap_or_default();
    let method = auth.and_then(|a| text_at(a, "method")).unwrap_or_else(|| {
        if token.is_empty() {
            "none".into()
        } else {
            "token".into()
        }
    });
    let auth_method = if method.eq_ignore_ascii_case("none") || token.is_empty() {
        "none".to_string()
    } else {
        "token".to_string()
    };

    let proxies_value = root
        .get("proxies")
        .and_then(Value::as_array)
        .ok_or("frpc.toml 里没有找到 [[proxies]] 代理配置")?;
    if proxies_value.is_empty() {
        return Err("frpc.toml 里没有代理配置".into());
    }
    if proxies_value.len() > MAX_PROXIES {
        return Err(format!("代理数量超过 {MAX_PROXIES} 条，已取消导入"));
    }

    let mut proxies = Vec::with_capacity(proxies_value.len());
    for (i, p) in proxies_value.iter().enumerate() {
        let where_ = format!("第 {} 条代理", i + 1);
        if !p.is_table() {
            return Err(format!("{where_}格式不正确"));
        }
        let name = text_at(p, "name").unwrap_or_else(|| format!("导入项目 {}", i + 1));
        let proxy_type = text_at(p, "type")
            .unwrap_or_else(|| "tcp".into())
            .to_ascii_lowercase();
        let proxy_type = match proxy_type.as_str() {
            "tcp" | "udp" | "http" | "https" | "stcp" | "xtcp" | "sudp" => proxy_type,
            _ => return Err(format!("{where_}的类型不支持：{proxy_type}")),
        };
        let local_port = int_at(p, "localPort").ok_or_else(|| format!("{where_}缺少 localPort"))?;
        if !(1..=65535).contains(&local_port) {
            return Err(format!("{where_}的 localPort 不合法：{local_port}"));
        }
        let local_ip = text_at(p, "localIP").unwrap_or_else(|| "127.0.0.1".into());

        let remote_port = if proxy_type == "tcp" || proxy_type == "udp" {
            int_at(p, "remotePort")
                .filter(|v| (1..=65535).contains(v))
                .map(|v| v as u32)
        } else {
            None
        };

        let mut custom_domains: Vec<String> = p
            .get("customDomains")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if custom_domains.is_empty() {
            if let Some(domain) = text_at(p, "customDomain") {
                custom_domains.push(domain);
            }
        }

        let http = p.get("httpUser");
        let http_user = http.and_then(|h| text_at(h, "user"));
        let http_password = http.and_then(|h| text_at(h, "password"));

        proxies.push(ImportedProxy {
            name,
            proxy_type,
            local_ip,
            local_port: local_port as u32,
            remote_port,
            custom_domains,
            subdomain: text_at(p, "subdomain"),
            host_header: text_at(p, "hostHeader"),
            http_user,
            http_password,
            secret_key: text_at(p, "secretKey"),
        });
    }

    Ok(ImportedFrpc {
        server_addr,
        server_port: server_port as u32,
        protocol,
        tls,
        auth_method,
        token,
        tcp_mux,
        pool_count,
        heartbeat_interval,
        proxies,
    })
}

/// 读取用户选择的 frpc.toml；只读取，不修改源文件。
#[tauri::command]
pub fn project_import_toml(path: String) -> Result<ImportedFrpc, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| format!("读取 frpc.toml 失败：{e}"))?;
    if !meta.is_file() {
        return Err("选择的不是文件".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("frpc.toml 超过 2 MB，已取消导入".into());
    }
    let text = fs::read_to_string(p).map_err(|e| format!("读取 frpc.toml 失败：{e}"))?;
    parse_frpc_toml(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_frpc_toml() {
        let text = r#"
serverAddr = "203.0.113.10"
serverPort = 7000
auth.method = "token"
auth.token = "secret"

[transport]
protocol = "tcp"
tcpMux = false
poolCount = 2
heartbeatInterval = 15

[transport.tls]
enable = false

[[proxies]]
name = "rdp"
type = "tcp"
localIP = "127.0.0.1"
localPort = 3389
remotePort = 6000

[[proxies]]
name = "web"
type = "https"
localIP = "127.0.0.1"
localPort = 8080
customDomains = ["app.example.com", "alt.example.com"]

[proxies.httpUser]
user = "admin"
password = "pw"
"#;
        let c = parse_frpc_toml(text).unwrap();
        assert_eq!(c.server_addr, "203.0.113.10");
        assert_eq!(c.server_port, 7000);
        assert_eq!(c.protocol, "tcp");
        assert!(!c.tls);
        assert_eq!(c.auth_method, "token");
        assert_eq!(c.token, "secret");
        assert!(!c.tcp_mux);
        assert_eq!(c.pool_count, 2);
        assert_eq!(c.heartbeat_interval, 15);
        assert_eq!(c.proxies.len(), 2);
        assert_eq!(c.proxies[0].proxy_type, "tcp");
        assert_eq!(c.proxies[0].remote_port, Some(6000));
        assert_eq!(c.proxies[1].custom_domains.len(), 2);
        assert_eq!(c.proxies[1].http_user.as_deref(), Some("admin"));
    }

    #[test]
    fn rejects_missing_proxies() {
        let err = parse_frpc_toml("serverAddr = \"x\"\nserverPort = 7000\n").unwrap_err();
        assert!(err.contains("[[proxies]]"));
    }
}
