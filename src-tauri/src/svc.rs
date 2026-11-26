//! 服务托管层：Windows 用 SCM（sc.exe），macOS 用 launchd（LaunchAgent），
//! Linux 用 systemd 用户单元（systemctl --user）。
//!
//! 前端只认这 5 个命令 + platform_info，后端按平台分派：
//! - Windows：服务宿主是本程序自身的 `--service-host` 模式（见 svc_host.rs），
//!   由 SCM 以 SYSTEM 身份拉起，因此安装/卸载需要管理员权限。
//! - macOS：plist 里直接执行 frpc，launchd 自己负责守护与重启，
//!   不需要包装进程，也不需要管理员权限（用户级 LaunchAgent）。
//! - Linux：unit 文件里直接执行 frpc，systemd 负责守护与重启；
//!   用用户级单元（`~/.config/systemd/user/`），同样不需要管理员权限。
//!   依赖 systemd 240+（`StandardOutput=append:`）；无 systemd 的会话会明确报错。
use serde::Serialize;

/// 各平台服务实现的统一返回
#[derive(Serialize)]
pub struct SvcResult {
    pub ok: bool,
    pub output: String,
}

impl SvcResult {
    #[allow(dead_code)]
    fn ok(output: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
        }
    }
    #[allow(dead_code)]
    fn not_installed() -> Self {
        Self {
            ok: false,
            output: "not-installed".into(),
        }
    }
}

/// 平台信息：前端据此显示「Windows 服务」/「LaunchAgent」等文案
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// windows / macos / linux
    pub os: &'static str,
    /// 平台自启形态，如「Windows 服务」「LaunchAgent」
    pub service_kind: &'static str,
    /// 服务标识字段的界面标签，如「服务名」「LaunchAgent」
    pub service_id_label: &'static str,
    /// 服务标识前缀（macOS 的 Label 前缀；Windows 为空）
    pub label_prefix: &'static str,
    /// 自启语义，如「系统启动时自动拉起」「登录时自动拉起」
    pub autostart: &'static str,
    /// 安装服务是否需要管理员权限
    pub needs_admin: bool,
    /// 服务定义存放位置（给界面展示）
    pub config_path: String,
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    imp::platform_info()
}

#[tauri::command]
pub fn service_install(
    name: String,
    display: String,
    description: String,
    id: String,
    version: String,
) -> Result<SvcResult, String> {
    imp::install(name, display, description, id, version)
}

#[tauri::command]
pub fn service_uninstall(name: String, id: Option<String>) -> Result<SvcResult, String> {
    imp::uninstall(&name, id.as_deref())
}

#[tauri::command]
pub fn service_control(
    name: String,
    action: String,
    id: Option<String>,
) -> Result<SvcResult, String> {
    imp::control(&name, id.as_deref(), &action)
}

#[tauri::command]
pub fn service_restart(name: String, id: Option<String>) -> Result<SvcResult, String> {
    imp::restart(&name, id.as_deref())
}

#[tauri::command]
pub fn service_query(name: String, id: Option<String>) -> Result<SvcResult, String> {
    imp::query(&name, id.as_deref())
}

/// 由 query 的输出判断托管进程是否在运行（自检用）
pub fn query_running(output: &str) -> bool {
    imp::is_running(output)
}

/// 停止时「本来就没运行」这类可以忽略的错误（自检用）
pub fn already_stopped(err: &str) -> bool {
    imp::already_stopped(err)
}

// ───────────────────────────── Windows（SCM） ─────────────────────────────
#[cfg(windows)]
mod imp {
    use super::{PlatformInfo, SvcResult};
    use crate::paths;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn platform_info() -> PlatformInfo {
        PlatformInfo {
            os: "windows",
            service_kind: "Windows 服务",
            service_id_label: "服务名",
            label_prefix: "",
            autostart: "系统启动时自动拉起",
            needs_admin: true,
            config_path: "Windows 服务管理器（sc.exe create）".into(),
        }
    }

    /// sc.exe 在中日文系统上输出 GBK，这里按 GBK 解码，失败再退 UTF-8
    fn decode(bytes: &[u8]) -> String {
        let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
        if had_errors {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            cow.into_owned()
        }
    }

    fn sc(args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new("sc.exe");
        cmd.args(args);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| format!("调用 sc.exe 失败：{e}"))?;
        let text = decode(&out.stdout) + &decode(&out.stderr);
        if !out.status.success() {
            let body = text.trim();
            let hint = if body.contains("拒绝访问") || body.to_lowercase().contains("access is denied")
            {
                "（需要管理员权限：先关闭本程序，再右键 → 以管理员身份运行）"
            } else {
                ""
            };
            return Err(format!(
                "sc {} 失败（退出码 {:?}）：{}{}",
                args.first().unwrap_or(&""),
                out.status.code(),
                body,
                hint
            ));
        }
        Ok(text)
    }

    fn host_bin(id: &str, name: &str, version: &str, toml: &str, log: &str) -> Result<String, String> {
        let host = std::env::current_exe().map_err(|e| format!("定位程序路径失败：{e}"))?;
        let frpc = paths::frpc_exe(version)?;
        Ok(format!(
            "\"{}\" --service-host --project \"{}\" --name \"{}\" --frpc \"{}\" --config \"{}\" --log \"{}\"",
            host.display(),
            id,
            name,
            frpc.display(),
            toml,
            log
        ))
    }

    pub fn install(
        name: String,
        display: String,
        description: String,
        id: String,
        version: String,
    ) -> Result<SvcResult, String> {
        let id = paths::safe_id(&id)?;
        let toml = paths::project_toml(&id)?;
        if !toml.exists() {
            return Err("还没有生成 frpc.toml，请先点一次「启动」或保存配置".into());
        }
        let log = paths::project_log(&id)?;
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let bin = host_bin(&id, &name, &version, &toml.display().to_string(), &log.display().to_string())?;

        // 已存在则先删掉，保证幂等
        let _ = sc(&["stop", &name]);
        let _ = sc(&["delete", &name]);

        let mut out = String::new();
        out += &sc(&[
            "create",
            &name,
            "binPath=",
            &bin,
            "start=",
            "auto",
            "DisplayName=",
            &display,
        ])?;
        let _ = sc(&["description", &name, &description]);
        let start = sc(&["start", &name]);
        out += &start.unwrap_or_else(|e| format!("\n启动返回：{e}"));
        Ok(SvcResult { ok: true, output: out })
    }

    pub fn uninstall(name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        let mut out = String::new();
        if let Ok(t) = sc(&["stop", name]) {
            out += &t;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        match sc(&["delete", name]) {
            Ok(t) => out += &t,
            Err(e) if e.contains("1060") || e.contains("指定的服务未安装") => {
                out += "[SC] 服务不存在，无需删除";
            }
            Err(e) => return Err(e),
        }
        Ok(SvcResult { ok: true, output: out })
    }

    pub fn control(name: &str, _id: Option<&str>, action: &str) -> Result<SvcResult, String> {
        let a = match action {
            "start" => "start",
            "stop" => "stop",
            other => return Err(format!("未知操作：{other}")),
        };
        let out = sc(&[a, name])?;
        Ok(SvcResult { ok: true, output: out })
    }

    pub fn restart(name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        let mut out = String::new();
        if let Ok(t) = sc(&["stop", name]) {
            out += &t;
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        out += &sc(&["start", name])?;
        Ok(SvcResult { ok: true, output: out })
    }

    /// 查询服务状态（服务不存在时返回 not-installed）
    pub fn query(name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        match sc(&["query", name]) {
            Ok(text) => Ok(SvcResult { ok: true, output: text }),
            Err(e) => {
                if e.contains("1060") || e.contains("指定的服务未安装") {
                    Ok(SvcResult::not_installed())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn is_running(output: &str) -> bool {
        output.contains("RUNNING")
    }

    pub fn already_stopped(err: &str) -> bool {
        err.contains("1062") || err.contains("服务没有启动") || err.contains("not active")
    }
}

// ───────────────────────────── macOS（launchd） ─────────────────────────────
#[cfg(target_os = "macos")]
mod imp {
    use super::{PlatformInfo, SvcResult};
    use crate::paths;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// LaunchAgent Label 前缀；完整 Label = `com.sxq.frptm.<项目id>`
    const LABEL_PREFIX: &str = "com.sxq.frptm";

    fn uid() -> u32 {
        // SAFETY: getuid 没有参数、没有副作用，永远成功
        unsafe { libc::getuid() }
    }

    fn domain() -> String {
        format!("gui/{}", uid())
    }

    fn service_target(label: &str) -> String {
        format!("{}/{}", domain(), label)
    }

    pub fn platform_info() -> PlatformInfo {
        PlatformInfo {
            os: "macos",
            service_kind: "LaunchAgent",
            service_id_label: "LaunchAgent",
            label_prefix: "com.sxq.frptm.",
            autostart: "登录时自动拉起",
            needs_admin: false,
            config_path: "~/Library/LaunchAgents/".into(),
        }
    }

    /// 去掉 plist 里非法/需要转义的字符（路径里最常见的是 &）
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn plist_xml(label: &str, frpc: &Path, toml: &Path, log: &Path) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{frpc}</string>
        <string>-c</string>
        <string>{toml}</string>
    </array>
    <!-- 登录即拉起；异常退出由 launchd 立刻重启，ThrottleInterval 防止 frpc 配置出错时疯狂重启 -->
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>3</integer>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
            label = xml(label),
            frpc = xml(&frpc.display().to_string()),
            toml = xml(&toml.display().to_string()),
            log = xml(&log.display().to_string()),
        )
    }

    fn launchctl(args: &[&str]) -> Result<String, String> {
        let out = Command::new("/bin/launchctl")
            .args(args)
            .output()
            .map_err(|e| format!("调用 launchctl 失败：{e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            let body = text.trim();
            let body = if body.is_empty() {
                format!("退出码 {:?}", out.status.code())
            } else {
                body.to_string()
            };
            return Err(format!("launchctl {} 失败：{}", args.join(" "), body));
        }
        Ok(text)
    }

    /// launchctl 报「服务不存在 / 没加载」——停止、查询时都当成正常状态
    fn not_loaded(e: &str) -> bool {
        let l = e.to_lowercase();
        l.contains("could not find service")
            || l.contains("no such process")
            || l.contains("not find")
            || l.contains("not loaded")
            || l.contains("service is not running")
    }

    /// macOS 26 上 `bootstrap gui/<uid>` 可能被拒（error 5 / 125），需要回退到 `load -w`
    fn bootstrap_unsupported(e: &str) -> bool {
        let l = e.to_lowercase();
        l.contains("input/output error")
            || l.contains("domain does not support")
            || l.contains("bootstrap failed: 5")
            || l.contains("bootstrap failed: 125")
            || l.contains(": 125:")
    }

    pub fn label_of(id: &str) -> Result<String, String> {
        Ok(format!("{LABEL_PREFIX}.{}", paths::safe_id(id)?))
    }

    /// 只有服务名、拿不到项目 id 时（旧数据/自检），从名字推导一个稳定 Label
    fn label_from_name(name: &str) -> String {
        let mut s = String::new();
        let mut dash = false;
        for c in name.chars() {
            if c.is_ascii_alphanumeric() {
                s.push(c.to_ascii_lowercase());
                dash = false;
            } else if !dash && !s.is_empty() {
                s.push('-');
                dash = true;
            }
        }
        let s = s.trim_matches('-');
        let s = if s.is_empty() { "project" } else { s };
        format!("{LABEL_PREFIX}.{s}")
    }

    fn label(name: &str, id: Option<&str>) -> String {
        match id.and_then(|i| label_of(i).ok()) {
            Some(l) => l,
            None => label_from_name(name),
        }
    }

    fn launch_agents_dir() -> Result<PathBuf, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| !h.as_os_str().is_empty())
            .ok_or_else(|| "读取 HOME 失败，找不到 ~/Library/LaunchAgents".to_string())?;
        Ok(home.join("Library").join("LaunchAgents"))
    }

    fn plist_path(label: &str) -> Result<PathBuf, String> {
        Ok(launch_agents_dir()?.join(format!("{label}.plist")))
    }

    fn write_plist(label: &str, frpc: &Path, toml: &Path, log: &Path) -> Result<PathBuf, String> {
        let dir = launch_agents_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", dir.display()))?;
        let path = dir.join(format!("{label}.plist"));
        let text = plist_xml(label, frpc, toml, log);
        // 先写临时文件再 rename：避免 launchd 读到写了一半的 plist
        let tmp = dir.join(format!(".{label}.plist.tmp"));
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("写入 plist 失败：{e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
        }
        std::fs::rename(&tmp, &path).map_err(|e| format!("安装 plist 失败：{e}"))?;
        // plutil 是系统自带的 plist 语法检查器，把「写错了」变成明确报错
        let ps = path.display().to_string();
        if let Ok(o) = Command::new("/usr/bin/plutil")
            .args(["-lint", ps.as_str()])
            .output()
        {
            if !o.status.success() {
                let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
                let _ = std::fs::remove_file(&path);
                return Err(format!("生成的 plist 校验失败：{msg}"));
            }
        }
        Ok(path)
    }

    fn bootout(label: &str) -> Result<String, String> {
        let target = service_target(label);
        match launchctl(&["bootout", &target]) {
            Ok(text) => Ok(text),
            Err(e) if not_loaded(&e) => Ok("服务未加载，跳过停止".into()),
            Err(e) => {
                if let Ok(p) = plist_path(label) {
                    if p.exists() {
                        let ps = p.display().to_string();
                        match launchctl(&["bootout", &domain(), &ps]) {
                            Ok(text) => return Ok(text),
                            Err(e2) if not_loaded(&e2) => return Ok("服务未加载，跳过停止".into()),
                            Err(e2) => return Err(format!("{e}；再按 plist 卸载也失败：{e2}")),
                        }
                    }
                }
                Err(e)
            }
        }
    }

    fn bootstrap(label: &str) -> Result<String, String> {
        let _ = launchctl(&["enable", &service_target(label)]);
        let plist = plist_path(label)?;
        let ps = plist.display().to_string();
        match launchctl(&["bootstrap", &domain(), &ps]) {
            Ok(text) => Ok(text),
            Err(e) if bootstrap_unsupported(&e) => {
                let text = launchctl(&["load", "-w", &ps])
                    .map_err(|e2| format!("{e}；回退 launchctl load 也失败：{e2}"))?;
                Ok(format!(
                    "{text}\n[提示] gui 域拒绝 bootstrap（{e}），已回退到 launchctl load。"
                ))
            }
            Err(e) => Err(e),
        }
    }

    pub fn install(
        _name: String,
        _display: String,
        _description: String,
        id: String,
        version: String,
    ) -> Result<SvcResult, String> {
        let label = label_of(&id)?;
        let toml = paths::project_toml(&id)?;
        if !toml.exists() {
            return Err("还没有生成 frpc.toml，请先点一次「启动」或保存配置".into());
        }
        let frpc = paths::frpc_exe(&version)?;
        if !frpc.exists() {
            return Err(format!(
                "frp {version} 还没安装，请先到「设置 → FRP 版本」里点「安装」"
            ));
        }
        let log = paths::project_log(&id)?;
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }

        let mut out = String::new();
        if let Ok(t) = bootout(&label) {
            out.push_str(t.trim());
            out.push('\n');
        }
        let plist = write_plist(&label, &frpc, &toml, &log)?;
        out.push_str(&bootstrap(&label)?);
        out.push_str(&format!("\nplist：{}", plist.display()));
        Ok(SvcResult::ok(out))
    }

    pub fn uninstall(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        let mut out = String::new();
        out.push_str(bootout(&label)?.trim());
        let p = plist_path(&label)?;
        if p.exists() {
            std::fs::remove_file(&p)
                .map_err(|e| format!("删除 plist 失败（{}）：{e}", p.display()))?;
            out.push_str(&format!("\n已删除 {}", p.display()));
        } else {
            out.push_str("\nplist 不存在，无需删除");
        }
        Ok(SvcResult::ok(out))
    }

    pub fn control(name: &str, id: Option<&str>, action: &str) -> Result<SvcResult, String> {
        let label = label(name, id);
        match action {
            "start" => {
                let p = plist_path(&label)?;
                if !p.exists() {
                    return Err("LaunchAgent 还没安装，请先点「安装服务」".into());
                }
                Ok(SvcResult::ok(bootstrap(&label)?))
            }
            // KeepAlive 的 job 直接 kill 会被 launchd 立刻重启，必须 bootout 才算停
            "stop" => Ok(SvcResult::ok(bootout(&label)?)),
            other => Err(format!("未知操作：{other}")),
        }
    }

    pub fn restart(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        let p = plist_path(&label)?;
        if !p.exists() {
            return Err("LaunchAgent 还没安装，请先点「安装服务」".into());
        }
        let mut out = String::new();
        if let Ok(t) = bootout(&label) {
            out.push_str(t.trim());
            out.push('\n');
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        out.push_str(&bootstrap(&label)?);
        Ok(SvcResult::ok(out))
    }

    pub fn query(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        match launchctl(&["print", &service_target(&label)]) {
            Ok(text) => Ok(SvcResult { ok: true, output: text }),
            Err(e) if not_loaded(&e) => {
                let p = plist_path(&label)?;
                if p.exists() {
                    Ok(SvcResult {
                        ok: false,
                        output: format!("not-loaded\nplist：{}", p.display()),
                    })
                } else {
                    Ok(SvcResult::not_installed())
                }
            }
            Err(e) => Err(e),
        }
    }

    pub fn is_running(output: &str) -> bool {
        output.contains("state = running") || output.contains("state = starting")
    }

    pub fn already_stopped(err: &str) -> bool {
        not_loaded(err)
    }
}

// ───────────────────────────── Linux（systemd 用户单元） ─────────────────────────────
#[cfg(target_os = "linux")]
mod imp {
    use super::{PlatformInfo, SvcResult};
    use crate::paths;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// 单元名前缀；完整单元名 = `com.sxq.frptm.<项目id>.service`
    const LABEL_PREFIX: &str = "com.sxq.frptm";

    fn uid() -> u32 {
        // SAFETY: getuid 没有参数、没有副作用，永远成功
        unsafe { libc::getuid() }
    }

    pub fn platform_info() -> PlatformInfo {
        PlatformInfo {
            os: "linux",
            service_kind: "systemd 用户服务",
            service_id_label: "systemd 单元",
            label_prefix: "com.sxq.frptm.",
            autostart: "登录时自动拉起",
            needs_admin: false,
            config_path: "~/.config/systemd/user/".into(),
        }
    }

    fn user_unit_dir() -> Result<PathBuf, String> {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
            if !dir.as_os_str().is_empty() {
                return Ok(dir.join("systemd").join("user"));
            }
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| !h.as_os_str().is_empty())
            .ok_or_else(|| "读取 HOME 失败，找不到 ~/.config/systemd/user".to_string())?;
        Ok(home.join(".config").join("systemd").join("user"))
    }

    fn unit_path(label: &str) -> Result<PathBuf, String> {
        Ok(user_unit_dir()?.join(format!("{label}.service")))
    }

    /// systemd 里 `%` 会被当成 specifier 展开，必须写成 `%%`
    fn pct(s: &str) -> String {
        s.replace('%', "%%")
    }

    /// ExecStart 的参数要按 systemd 的词法加引号，否则路径里有空格就被拆开
    fn unit_quote(s: &str) -> String {
        let bs = char::from(92);
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            if c == '%' {
                out.push_str("%%");
            } else if c == '"' || c == bs {
                out.push(bs);
                out.push(c);
            } else {
                out.push(c);
            }
        }
        out.push('"');
        out
    }

    fn unit_text(display: &str, frpc: &Path, toml: &Path, log: &Path) -> String {
        let desc = display.lines().collect::<Vec<_>>().join(" ");
        let desc = pct(desc.trim());
        let workdir = toml.parent().map(Path::to_path_buf).unwrap_or_default();
        let frpc_s = unit_quote(&frpc.display().to_string());
        let toml_s = unit_quote(&toml.display().to_string());
        let log_s = pct(&log.display().to_string());
        let mut s = String::new();
        let _ = writeln!(s, "[Unit]");
        let _ = writeln!(s, "Description={desc}");
        let _ = writeln!(s, "After=network-online.target");
        let _ = writeln!(s, "Wants=network-online.target");
        // 配置写错时 frpc 会立刻退出：关掉启动频率限制，让 RestartSec 一直生效
        let _ = writeln!(s, "StartLimitIntervalSec=0");
        let _ = writeln!(s);
        let _ = writeln!(s, "[Service]");
        let _ = writeln!(s, "Type=simple");
        let _ = writeln!(s, "ExecStart={frpc_s} -c {toml_s}");
        let _ = writeln!(s, "WorkingDirectory={}", pct(&workdir.display().to_string()));
        let _ = writeln!(s, "Restart=always");
        let _ = writeln!(s, "RestartSec=3");
        // systemd 240+ 支持 append:；目标桌面（WebKitGTK 4.1）都满足
        let _ = writeln!(s, "StandardOutput=append:{log_s}");
        let _ = writeln!(s, "StandardError=append:{log_s}");
        let _ = writeln!(s);
        let _ = writeln!(s, "[Install]");
        let _ = writeln!(s, "WantedBy=default.target");
        s
    }

    fn write_unit(
        label: &str,
        display: &str,
        frpc: &Path,
        toml: &Path,
        log: &Path,
    ) -> Result<PathBuf, String> {
        let dir = user_unit_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", dir.display()))?;
        let path = dir.join(format!("{label}.service"));
        let text = unit_text(display, frpc, toml, log);
        // 先写临时文件再 rename：避免 systemd 读到写了一半的 unit
        let tmp = dir.join(format!(".{label}.service.tmp"));
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("写入 unit 文件失败：{e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("安装 unit 文件失败：{e}"))?;
        Ok(path)
    }

    /// 当前会话连不上 systemd 用户实例（容器 / WSL / 非 systemd 发行版）
    fn no_manager(e: &str) -> bool {
        let l = e.to_lowercase();
        l.contains("failed to connect to bus")
            || l.contains("failed to connect to socket")
            || l.contains("cannot autolaunch dbus")
            || l.contains("system has not been booted with systemd")
            || l.contains("systemd is not running")
    }

    fn fail(args: &[&str], body: &str, code: Option<i32>) -> String {
        let body = if body.is_empty() {
            format!("退出码 {code:?}")
        } else {
            body.to_string()
        };
        let mut msg = format!("systemctl {} 失败：{body}", args.join(" "));
        if no_manager(&body) {
            msg.push_str("（当前会话连不上 systemd 用户实例：容器 / WSL / 非 systemd 发行版无法使用服务托管，请改用「直接运行」）");
        }
        msg
    }

    fn systemctl(args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new("systemctl");
        cmd.arg("--user").args(args).env("LC_ALL", "C");
        // GUI 启动的进程有时拿不到这两个变量，补上桌面会话的常见默认值
        if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
            let d = PathBuf::from(format!("/run/user/{}", uid()));
            if d.is_dir() {
                cmd.env("XDG_RUNTIME_DIR", &d);
            }
        }
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
            let b = PathBuf::from(format!("/run/user/{}/bus", uid()));
            if b.exists() {
                cmd.env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={}", b.display()));
            }
        }
        let out = cmd.output().map_err(|e| {
            format!("调用 systemctl 失败：{e}。请确认当前系统使用 systemd 且已安装 systemctl。")
        })?;
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        let body = text.trim().to_string();
        if out.status.success() {
            Ok(body)
        } else {
            Err(fail(args, &body, out.status.code()))
        }
    }

    fn not_loaded(e: &str) -> bool {
        let l = e.to_lowercase();
        l.contains("not loaded")
            || l.contains("could not be found")
            || l.contains("not found")
            || l.contains("no such file")
    }

    pub fn label_of(id: &str) -> Result<String, String> {
        Ok(format!("{LABEL_PREFIX}.{}", paths::safe_id(id)?))
    }

    /// 只有服务名、拿不到项目 id 时（旧数据/自检），从名字推导一个稳定单元名
    fn label_from_name(name: &str) -> String {
        let mut s = String::new();
        let mut dash = false;
        for c in name.chars() {
            if c.is_ascii_alphanumeric() {
                s.push(c.to_ascii_lowercase());
                dash = false;
            } else if !dash && !s.is_empty() {
                s.push('-');
                dash = true;
            }
        }
        let s = s.trim_matches('-');
        let s = if s.is_empty() { "project" } else { s };
        format!("{LABEL_PREFIX}.{s}")
    }

    fn label(name: &str, id: Option<&str>) -> String {
        match id.and_then(|i| label_of(i).ok()) {
            Some(l) => l,
            None => label_from_name(name),
        }
    }

    pub fn install(
        _name: String,
        display: String,
        _description: String,
        id: String,
        version: String,
    ) -> Result<SvcResult, String> {
        let label = label_of(&id)?;
        let toml = paths::project_toml(&id)?;
        if !toml.exists() {
            return Err("还没有生成 frpc.toml，请先点一次「启动」或保存配置".into());
        }
        let frpc = paths::frpc_exe(&version)?;
        if !frpc.exists() {
            return Err(format!(
                "frp {version} 还没安装，请先到「设置 → FRP 版本」里点「安装」"
            ));
        }
        let log = paths::project_log(&id)?;
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let unit = write_unit(&label, &display, &frpc, &toml, &log)?;
        // 先 daemon-reload 让新单元可见，再 enable（登录自启）+ restart（立刻启动）
        let _ = systemctl(&["daemon-reload"]);
        systemctl(&["enable", label.as_str()])?;
        systemctl(&["restart", label.as_str()])?;
        let mut out = String::new();
        let _ = writeln!(out, "已安装 systemd 用户单元：{}", unit.display());
        let _ = writeln!(out, "自启：登录后由 systemd 拉起（WantedBy=default.target）");
        Ok(SvcResult { ok: true, output: out })
    }

    pub fn uninstall(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        let p = unit_path(&label)?;
        let mut out = String::new();
        // 不管文件在不在都先停掉并取消自启：单元可能还留在管理器里加载着
        let _ = systemctl(&["disable", "--now", label.as_str()]);
        if p.exists() {
            std::fs::remove_file(&p)
                .map_err(|e| format!("删除 unit 文件失败（{}）：{e}", p.display()))?;
            let _ = systemctl(&["daemon-reload"]);
            let _ = writeln!(out, "已删除 {}", p.display());
        } else {
            let _ = writeln!(out, "unit 文件不存在，无需删除");
        }
        Ok(SvcResult { ok: true, output: out })
    }

    pub fn control(name: &str, id: Option<&str>, action: &str) -> Result<SvcResult, String> {
        let label = label(name, id);
        match action {
            "start" => {
                let p = unit_path(&label)?;
                if !p.exists() {
                    return Err("systemd 服务还没安装，请先点「安装服务」".into());
                }
                let _ = systemctl(&["daemon-reload"]);
                Ok(SvcResult::ok(systemctl(&["start", label.as_str()])?))
            }
            "stop" => match systemctl(&["stop", label.as_str()]) {
                Ok(t) => Ok(SvcResult::ok(t)),
                Err(e) if already_stopped(&e) => Ok(SvcResult::ok("服务未加载，跳过停止")),
                Err(e) => Err(e),
            },
            other => Err(format!("未知操作：{other}")),
        }
    }

    pub fn restart(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        let p = unit_path(&label)?;
        if !p.exists() {
            return Err("systemd 服务还没安装，请先点「安装服务」".into());
        }
        let _ = systemctl(&["daemon-reload"]);
        Ok(SvcResult::ok(systemctl(&["restart", label.as_str()])?))
    }

    pub fn query(name: &str, id: Option<&str>) -> Result<SvcResult, String> {
        let label = label(name, id);
        let p = unit_path(&label)?;
        let text = match systemctl(&[
            "show",
            "-p",
            "LoadState",
            "-p",
            "ActiveState",
            "-p",
            "SubState",
            "-p",
            "MainPID",
            "-p",
            "UnitFileState",
            label.as_str(),
        ]) {
            Ok(t) => t,
            Err(e) if not_loaded(&e) => String::new(),
            Err(e) => return Err(e),
        };
        if text.contains("LoadState=loaded") {
            return Ok(SvcResult { ok: true, output: text });
        }
        if p.exists() {
            Ok(SvcResult {
                ok: false,
                output: format!("not-loaded\n单元文件：{}", p.display()),
            })
        } else {
            Ok(SvcResult::not_installed())
        }
    }

    pub fn is_running(output: &str) -> bool {
        output.lines().any(|l| {
            let l = l.trim();
            l == "ActiveState=active" || l == "ActiveState=activating"
        })
    }

    pub fn already_stopped(err: &str) -> bool {
        not_loaded(err)
    }
}

// ───────────────────────────── 其它平台 ─────────────────────────────
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod imp {
    use super::{PlatformInfo, SvcResult};

    const UNSUPPORTED: &str =
        "当前系统不支持服务托管：Windows 用 SCM，macOS 用 launchd，Linux 用 systemd。";

    pub fn platform_info() -> PlatformInfo {
        PlatformInfo {
            os: "unknown",
            service_kind: "不受支持",
            service_id_label: "服务",
            label_prefix: "",
            autostart: "不支持开机自启",
            needs_admin: false,
            config_path: String::new(),
        }
    }

    pub fn install(
        _name: String,
        _display: String,
        _description: String,
        _id: String,
        _version: String,
    ) -> Result<SvcResult, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn uninstall(_name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn control(_name: &str, _id: Option<&str>, _action: &str) -> Result<SvcResult, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn restart(_name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn query(_name: &str, _id: Option<&str>) -> Result<SvcResult, String> {
        Ok(SvcResult::not_installed())
    }

    pub fn is_running(_output: &str) -> bool {
        false
    }

    pub fn already_stopped(_err: &str) -> bool {
        false
    }
}
