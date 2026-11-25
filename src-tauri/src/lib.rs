// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod paths;
pub mod proc;
pub mod svc;
pub mod versions;

#[cfg(windows)]
pub mod svc_host;

use std::collections::HashMap;
use tauri::Manager;

#[tauri::command]
fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        return matches!(
            Command::new("fltmc").creation_flags(0x0800_0000).output(),
            Ok(o) if o.status.success()
        );
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[tauri::command]
fn relaunch_elevated(app: tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("定位程序路径失败：{e}"))?;
    let script = format!("Start-Process -FilePath '{}' -Verb RunAs", exe.display());
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| format!("启动管理员实例失败：{e}"))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]

fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 通过 Rust 侧发起 GET（用于拉 GitHub Releases；支持 HTTP 代理）
#[tauri::command]
fn http_get_text(url: String, proxy: Option<String>) -> Result<String, String> {
    let mut agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20));
    if let Some(p) = proxy.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        let parsed = ureq::Proxy::new(p).map_err(|e| format!("代理地址无效：{e}"))?;
        agent = agent.proxy(parsed);
    }
    let resp = agent
        .build()
        .get(&url)
        .set("User-Agent", "FRP-Tunnel-Manager")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("请求失败：{e}"))?;
    resp.into_string().map_err(|e| format!("读取响应失败：{e}"))
}

/// 便携存储：<exe 所在目录>/config/store.json
fn store_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("定位程序目录失败：{e}"))?;
    let dir = exe.parent().ok_or("定位程序目录失败")?.join("config");
    Ok(dir.join("store.json"))
}

/// 读取本地数据；文件不存在时返回空字符串
#[tauri::command]
fn read_store() -> Result<String, String> {
    let p = store_path()?;
    match std::fs::read_to_string(&p) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("读取 {} 失败：{e}", p.display())),
    }
}

/// 原子写入本地数据，返回实际落盘路径
#[tauri::command]
fn write_store(json: String) -> Result<String, String> {
    let p = store_path()?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("写入失败：{e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("保存失败：{e}"))?;
    Ok(p.display().to_string())
}

#[cfg(windows)]
mod rdp {
    use super::HashMap;
    use winreg::enums::*;
    use winreg::RegKey;

    const ROOT: &str = r"SOFTWARE\\Policies\\Microsoft\\Windows NT\\Terminal Services";

    const KEYS: [&str; 12] = [
        "bEnumerateHWBeforeSW",
        "AVCHardwareEncodePreferred",
        "AVC444ModePreferred",
        "fEnableWddmDriver",
        "MaxCompressionLevel",
        "GraphicsProfile",
        "ImageQuality",
        "SelectTransport",
        "SelectNetworkDetect",
        "fNoRemoteDesktopWallpaper",
        "fEnableRemoteFXAdvancedRemoteApp",
        "UseShellAppRuntimeRemoteApp",
    ];

    /// 读取当前生效的远程桌面组策略（只返回已配置的键）
    #[tauri::command]
    pub fn rdp_policy_get() -> Result<HashMap<String, i64>, String> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let key = match hklm.open_subkey_with_flags(ROOT, KEY_READ) {
            Ok(k) => k,
            Err(_) => return Ok(HashMap::new()),
        };
        let mut out = HashMap::new();
        for name in KEYS {
            if let Ok(v) = key.get_value::<u32, _>(name) {
                out.insert(name.to_string(), v as i64);
            }
        }
        Ok(out)
    }

    /// 写入一条策略；value=None 表示删除该值（恢复「未配置」）
    #[tauri::command]
    pub fn rdp_policy_set(name: String, value: Option<i64>) -> Result<(), String> {
        if !KEYS.contains(&name.as_str()) {
            return Err(format!("未知的策略名：{name}"));
        }
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let key = match hklm.open_subkey_with_flags(ROOT, KEY_WRITE | KEY_READ) {
            Ok(k) => k,
            Err(_) => hklm
                .create_subkey(ROOT)
                .map(|(k, _)| k)
                .map_err(|e| format!("打开注册表失败（需要管理员权限）：{e}"))?,
        };
        match value {
            Some(v) => key
                .set_value(&name, &(v as u32))
                .map_err(|e| format!("写入失败（需要管理员权限）：{e}")),
            None => {
                let _ = key.delete_value(&name);
                Ok(())
            }
        }
    }
}

#[cfg(not(windows))]
mod rdp {
    use super::HashMap;

    #[tauri::command]
    pub fn rdp_policy_get() -> Result<HashMap<String, i64>, String> {
        Ok(HashMap::new())
    }

    #[tauri::command]
    pub fn rdp_policy_set(_name: String, _value: Option<i64>) -> Result<(), String> {
        Err("仅支持 Windows".into())
    }
}

/// CLI 自检：`--verify-service <out.json>`
/// 走一遍完整服务生命周期（查询→安装→查询→停止→卸载→查询），每步结果写进 JSON 报告。
/// 需要管理员权限；不创建任何窗口，供无 CDP/无界面时验证 M3 成功路径。
pub fn verify_service(out_path: &str) {
    fn push(steps: &mut Vec<serde_json::Value>, name: &str, ok: bool, output: &str) {
        steps.push(serde_json::json!({ "name": name, "ok": ok, "output": output }));
    }
    fn push_res(
        steps: &mut Vec<serde_json::Value>,
        name: &str,
        r: &Result<svc::SvcResult, String>,
    ) -> bool {
        let (ok, out) = match r {
            Ok(s) => (s.ok, s.output.clone()),
            Err(e) => (false, e.clone()),
        };
        push(steps, name, ok, &out);
        ok
    }
    fn write_report(out_path: &str, ok: bool, service: &str, steps: &[serde_json::Value]) {
        let report = serde_json::json!({ "ok": ok, "service": service, "steps": steps });
        let text = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into());
        let _ = std::fs::write(out_path, &text);
        println!("{text}");
    }

    let mut steps: Vec<serde_json::Value> = Vec::new();
    let mut ok = true;

    // 选一个已生成 frpc.toml 的项目
    let picked: Option<(String, String, String)> = (|| {
        let store = paths::config_dir().ok()?.join("store.json");
        let text = std::fs::read_to_string(store).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let arr = v.get("projects")?.as_array()?;
        for p in arr {
            let id = p.get("id")?.as_str()?.to_string();
            if !paths::project_toml(&id).map(|t| t.exists()).unwrap_or(false) {
                continue;
            }
            let name = p.get("serviceName")?.as_str()?.to_string();
            let ver = p.get("version")?.as_str().unwrap_or("").to_string();
            return Some((id, name, ver));
        }
        None
    })();
    let (id, name, version) = match picked {
        Some(x) => x,
        None => {
            push(
                &mut steps,
                "find-project",
                false,
                "store.json 里没有已生成 frpc.toml 的项目",
            );
            write_report(out_path, false, "-", &steps);
            return;
        }
    };

    let elevated = is_elevated();
    push(
        &mut steps,
        "is-elevated",
        elevated,
        if elevated { "管理员" } else { "非管理员" },
    );
    if !elevated {
        ok = false;
    }

    let q0 = svc::service_query(name.clone());
    push_res(&mut steps, "query-before", &q0);

    let inst = svc::service_install(
        name.clone(),
        name.clone(),
        format!("FRP Tunnel · {id}"),
        id.clone(),
        version.clone(),
    );
    if !push_res(&mut steps, "install", &inst) {
        ok = false;
    }

    std::thread::sleep(std::time::Duration::from_millis(1500));
    let q1 = svc::service_query(name.clone());
    let exists = matches!(&q1, Ok(s) if s.ok && s.output.contains("STATE"));
    push_res(&mut steps, "query-after-install", &q1);
    if !exists {
        ok = false;
    }

    let stop = svc::service_control(name.clone(), "stop".into());
    match &stop {
        Ok(s) => push(&mut steps, "stop", true, &s.output),
        Err(e) if e.contains("1062") || e.contains("服务没有启动") || e.contains("not active") => {
            push(&mut steps, "stop", true, "服务已是停止状态，跳过");
        }
        Err(e) => push(&mut steps, "stop", false, e),
    }
    std::thread::sleep(std::time::Duration::from_millis(800));

    let un = svc::service_uninstall(name.clone());
    if !push_res(&mut steps, "uninstall", &un) {
        ok = false;
    }
    std::thread::sleep(std::time::Duration::from_millis(600));

    let q2 = svc::service_query(name.clone());
    let gone = matches!(&q2, Ok(s) if !s.ok && s.output == "not-installed");
    match &q2 {
        Ok(s) => push(&mut steps, "query-after-uninstall", gone, &s.output),
        Err(e) => push(&mut steps, "query-after-uninstall", false, e),
    }
    if !gone {
        ok = false;
    }

    write_report(out_path, ok, &name, &steps);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(proc::Procs::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            is_elevated,
            relaunch_elevated,
            http_get_text,
            read_store,
            write_store,
            rdp::rdp_policy_get,
            rdp::rdp_policy_set,
            versions::version_install,
            versions::version_list_local,
            versions::version_remove,
            proc::project_write_toml,
            proc::proc_start,
            proc::proc_stop,
            proc::proc_info,
            proc::proc_logs,
            proc::project_read_log,
            proc::frpc_status,
            svc::service_install,
            svc::service_uninstall,
            svc::service_control,
            svc::service_restart,
            svc::service_query
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            let procs = handle.state::<proc::Procs>();
            proc::stop_all(&handle, &procs);
        }
    });
}
