//! 真实进程层：写 frpc.toml、spawn frpc.exe、转发日志、查询状态。
use crate::paths;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const MAX_LOG: usize = 800;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Entry {
    pub child: Child,
    pub pid: u32,
    pub started: Instant,
    pub log: Arc<Mutex<VecDeque<String>>>,
}

#[derive(Default)]
pub struct Procs(pub Mutex<HashMap<String, Entry>>);

#[derive(Serialize, Clone)]
pub struct ProcInfo {
    pub running: bool,
    pub pid: Option<u32>,
    pub uptime: u64,
}

#[derive(Serialize, Clone)]
pub struct LogLine {
    pub id: String,
    pub line: String,
}

#[derive(Serialize, Clone)]
pub struct ExitEvent {
    pub id: String,
}

/// 去掉 frpc 输出里的 ANSI 颜色控制序列
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if let Some('[') = chars.peek() {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn read_into_log(
    app: AppHandle,
    id: String,
    stream: impl std::io::Read + Send + 'static,
    log: Arc<Mutex<VecDeque<String>>>,
    log_path: Option<std::path::PathBuf>,
) {
    std::thread::spawn(move || {
        use std::io::Write;
        let mut file = log_path.and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            let line = strip_ansi(&line);
            if let Some(f) = file.as_mut() {
                let _ = writeln!(f, "{line}");
            }
            {
                let mut l = log.lock().unwrap();
                if l.len() >= MAX_LOG {
                    l.pop_front();
                }
                l.push_back(line.clone());
            }
            let _ = app.emit(
                "frpc-log",
                LogLine {
                    id: id.clone(),
                    line,
                },
            );
        }
    });
}

/// 把当前项目的 toml 写到 projects/<id>/frpc.toml，返回路径
#[tauri::command]
pub fn project_write_toml(id: String, toml: String) -> Result<String, String> {
    let p = paths::project_toml(&id)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建项目目录失败：{e}"))?;
    }
    std::fs::write(&p, toml.as_bytes()).map_err(|e| format!("写入 frpc.toml 失败：{e}"))?;
    Ok(p.display().to_string())
}

/// 读取项目历史日志（frpc.log 最后 200 行），进程没在跑时也能看
#[tauri::command]
pub fn project_read_log(id: String) -> Result<Vec<String>, String> {
    let id = paths::safe_id(&id)?;
    let p = paths::project_log(&id)?;
    if !p.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("读取日志失败：{e}"))?;
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let start = lines.len().saturating_sub(200);
    Ok(lines[start..].to_vec())
}

/// 启动项目：写 toml → 校验 frpc.exe → spawn → 管道日志
#[tauri::command]
pub fn proc_start(
    app: AppHandle,
    state: State<'_, Procs>,
    id: String,
    name: String,
    version: String,
    toml: String,
) -> Result<ProcInfo, String> {
    let id = paths::safe_id(&id)?;
    let exe = paths::frpc_exe(&version)?;
    if !exe.exists() {
        return Err(format!(
            "frp {version} 还没安装，请先到设置 → FRP 版本里点「安装」"
        ));
    }
    let toml_path = paths::project_toml(&id)?;
    if let Some(dir) = toml_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建项目目录失败：{e}"))?;
    }
    std::fs::write(&toml_path, toml.as_bytes()).map_err(|e| format!("写入 frpc.toml 失败：{e}"))?;

    // 已经在跑就先停掉
    {
        let mut map = state.0.lock().unwrap();
        if let Some(mut old) = map.remove(&id) {
            let _ = old.child.kill();
            let _ = old.child.wait();
        }
    }

    let mut cmd = Command::new(&exe);
    cmd.arg("-c")
        .arg(&toml_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 frpc 失败（{}）：{e}", exe.display()))?;
    let pid = child.id();
    let log: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let log_path = paths::project_log(&id).ok();
    if let Some(lp) = &log_path {
        let _ = std::fs::write(lp, b"");
    }
    if let Some(out) = child.stdout.take() {
        read_into_log(app.clone(), id.clone(), out, log.clone(), log_path.clone());
    }
    if let Some(err) = child.stderr.take() {
        read_into_log(app.clone(), id.clone(), err, log.clone(), log_path.clone());
    }
    let _ = name; // 名字只用于服务，这里保留参数方便以后写日志头
    let entry = Entry {
        child,
        pid,
        started: Instant::now(),
        log,
    };
    state.0.lock().unwrap().insert(id.clone(), entry);

    Ok(ProcInfo {
        running: true,
        pid: Some(pid),
        uptime: 0,
    })
}

#[tauri::command]
pub fn proc_stop(app: AppHandle, state: State<'_, Procs>, id: String) -> Result<bool, String> {
    let mut map = state.0.lock().unwrap();
    if let Some(mut e) = map.remove(&id) {
        let _ = e.child.kill();
        let _ = e.child.wait();
        let _ = app.emit("frpc-exit", ExitEvent { id });
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
pub fn proc_info(state: State<'_, Procs>, id: String) -> Result<ProcInfo, String> {
    let mut map = state.0.lock().unwrap();
    if let Some(e) = map.get_mut(&id) {
        match e.child.try_wait() {
            Ok(None) => {
                return Ok(ProcInfo {
                    running: true,
                    pid: Some(e.pid),
                    uptime: e.started.elapsed().as_secs(),
                })
            }
            _ => {
                map.remove(&id);
            }
        }
    }
    Ok(ProcInfo {
        running: false,
        pid: None,
        uptime: 0,
    })
}

#[tauri::command]
pub fn proc_logs(state: State<'_, Procs>, id: String) -> Result<Vec<String>, String> {
    let map = state.0.lock().unwrap();
    Ok(map
        .get(&id)
        .map(|e| e.log.lock().unwrap().iter().cloned().collect())
        .unwrap_or_default())
}

/// 停掉所有进程（应用退出时用）
pub fn stop_all(app: &AppHandle, state: &Procs) {
    let mut map = state.0.lock().unwrap();
    for (id, mut e) in map.drain() {
        let _ = e.child.kill();
        let _ = e.child.wait();
        let _ = app.emit("frpc-exit", ExitEvent { id });
    }
}

/// 读取 frpc 管理接口的 /api/status（用于判断隧道是否真的连上）
#[tauri::command]
pub fn frpc_status(port: u16) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(800))
        .timeout_read(std::time::Duration::from_millis(1200))
        .build();
    let url = format!("http://127.0.0.1:{port}/api/status");
    let resp = agent
        .get(&url)
        .set("User-Agent", "FRP-Tunnel-Manager")
        .call()
        .map_err(|e| format!("{e}"))?;
    resp.into_string().map_err(|e| format!("读取状态失败：{e}"))
}
