use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct ProxyServer {
    process: Mutex<Option<Child>>,
    auth_process: Mutex<Option<Child>>, // 跟踪认证进程
    proxy_port: u16,
    backend_port: u16,
    logs: Mutex<Vec<String>>,
    binary_path: Mutex<Option<PathBuf>>,
    config_path: Mutex<Option<PathBuf>>,
    pid_file_path: Mutex<Option<PathBuf>>,
}

impl ProxyServer {
    pub fn new(proxy_port: u16, backend_port: u16) -> Self {
        Self {
            process: Mutex::new(None),
            auth_process: Mutex::new(None),
            proxy_port,
            backend_port,
            logs: Mutex::new(Vec::new()),
            binary_path: Mutex::new(None),
            config_path: Mutex::new(None),
            pid_file_path: Mutex::new(None),
        }
    }

    /// 取消正在进行的认证进程
    pub fn cancel_auth(&self) {
        if let Ok(mut auth) = self.auth_process.lock() {
            if let Some(mut child) = auth.take() {
                let pid = child.id();
                self.add_log(format!("⚠️ Cancelling auth process (PID: {})...", pid));
                let _ = child.kill();
                let _ = child.wait();
                self.add_log("✓ Auth process cancelled".into());
            }
        }
    }

    pub fn set_binary_path(&self, path: PathBuf) {
        if let Ok(mut p) = self.binary_path.lock() {
            *p = Some(path);
        }
    }

    pub fn set_config_path(&self, path: PathBuf) {
        if let Ok(mut p) = self.config_path.lock() {
            *p = Some(path);
        }
    }

    pub fn set_pid_file_path(&self, path: PathBuf) {
        if let Ok(mut p) = self.pid_file_path.lock() {
            *p = Some(path);
        }
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.lock().ok().and_then(|p| p.clone())
    }

    pub fn has_binary(&self) -> bool {
        let result = self
            .binary_path
            .lock()
            .ok()
            .and_then(|p| {
                p.as_ref().map(|path| {
                    let exists = path.exists();
                    log::info!(
                        "[Server] has_binary check: path={:?}, exists={}",
                        path,
                        exists
                    );
                    exists
                })
            })
            .unwrap_or(false);
        log::info!("[Server] has_binary returning: {}", result);
        result
    }

    fn add_log(&self, msg: String) {
        if let Ok(mut logs) = self.logs.lock() {
            let ts = chrono::Local::now().format("%H:%M:%S");
            logs.push(format!("[{}] {}", ts, msg));
            if logs.len() > 500 {
                logs.remove(0);
            }
        }
        log::info!("{}", msg);
    }

    pub fn get_logs(&self) -> Vec<String> {
        self.logs.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn clear_logs(&self) {
        if let Ok(mut l) = self.logs.lock() {
            l.clear();
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut proc = self.process.lock().map_err(|e| e.to_string())?;
        if proc.is_some() {
            return Err("Server already running".into());
        }

        let bin = self.binary_path.lock().ok().and_then(|p| p.clone());
        let cfg = self.config_path.lock().ok().and_then(|p| p.clone());

        if bin.is_none() || cfg.is_none() || !bin.as_ref().unwrap().exists() {
            self.add_log("⚠️ Using simulation mode (binary not found)".into());
            self.add_log(format!(
                "✓ Backend server started in simulation mode (proxy {} -> backend {})",
                self.proxy_port, self.backend_port
            ));
            return Ok(());
        }

        let bin = bin.unwrap();
        let cfg = cfg.unwrap();
        self.cleanup_recorded_backend_process(&bin);

        self.add_log(format!(
            "Starting backend server on port {} (proxy port {})...",
            self.backend_port, self.proxy_port
        ));

        let mut cmd = Command::new(&bin);
        cmd.arg("-config")
            .arg(&cfg)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| {
            #[cfg(windows)]
            if e.raw_os_error() == Some(216) {
                return format!(
                    "Spawn failed: {}. The bundled cli-proxy-api.exe is likely built for a different CPU architecture than this machine. Rebuild the backend for the current host with `make build-cli-proxy` or rerun `make dev`.",
                    e
                );
            }

            format!("Spawn failed: {}", e)
        })?;

        let pid = child.id();
        self.record_backend_pid(pid);
        self.add_log(format!("✓ Server started (PID: {})", pid));

        if let Some(out) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(out).lines().flatten() {
                    log::info!("[srv] {}", line);
                }
            });
        }
        if let Some(err) = child.stderr.take() {
            thread::spawn(move || {
                for line in BufReader::new(err).lines().flatten() {
                    log::warn!("[srv-err] {}", line);
                }
            });
        }
        *proc = Some(child);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut proc = self.process.lock().map_err(|e| e.to_string())?;
        if let Some(mut c) = proc.take() {
            self.add_log(format!("Stopping server (PID: {})...", c.id()));
            let _ = c.kill();
            let _ = c.wait();
            self.clear_backend_pid();
            self.add_log("✓ Server stopped".into());
        }
        Ok(())
    }

    pub fn proxy_port(&self) -> u16 {
        self.proxy_port
    }

    pub fn backend_port(&self) -> u16 {
        self.backend_port
    }

    fn pid_file_path(&self) -> Option<PathBuf> {
        self.pid_file_path.lock().ok().and_then(|p| p.clone())
    }

    fn record_backend_pid(&self, pid: u32) {
        let Some(path) = self.pid_file_path() else {
            return;
        };

        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                log::warn!("Failed to create backend PID directory: {}", err);
                return;
            }
        }

        if let Err(err) = fs::write(&path, pid.to_string()) {
            log::warn!("Failed to write backend PID file {:?}: {}", path, err);
        }
    }

    fn clear_backend_pid(&self) {
        let Some(path) = self.pid_file_path() else {
            return;
        };

        if let Err(err) = fs::remove_file(&path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to remove backend PID file {:?}: {}", path, err);
            }
        }
    }

    fn cleanup_recorded_backend_process(&self, expected_binary: &Path) {
        let Some(path) = self.pid_file_path() else {
            return;
        };

        let pid_text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Failed to read backend PID file {:?}: {}", path, err);
                    self.clear_backend_pid();
                }
                return;
            }
        };

        let Ok(pid) = pid_text.trim().parse::<u32>() else {
            log::warn!("Ignoring invalid backend PID file {:?}", path);
            self.clear_backend_pid();
            return;
        };

        if pid == std::process::id() {
            self.clear_backend_pid();
            return;
        }

        let Some(actual_binary) = process_executable_path(pid) else {
            self.clear_backend_pid();
            return;
        };

        if !same_executable_path(&actual_binary, expected_binary) {
            log::warn!(
                "Skipping backend cleanup for PID {} because executable path does not match: {:?}",
                pid,
                actual_binary
            );
            self.clear_backend_pid();
            return;
        }

        self.add_log(format!(
            "Cleaning previous backend process (PID: {})...",
            pid
        ));
        terminate_process(pid);
        wait_until_process_exits(pid, std::time::Duration::from_millis(1500));
        if process_executable_path(pid).is_some() {
            force_terminate_process(pid);
            wait_until_process_exits(pid, std::time::Duration::from_millis(1500));
        }
        self.clear_backend_pid();
    }

    /// 运行认证命令，返回 (成功, 消息, 可选的设备码)
    pub fn run_auth_command(
        &self,
        svc: &str,
        qwen_email: Option<&str>,
    ) -> Result<AuthResult, String> {
        // 先取消之前的认证进程（如果有）
        self.cancel_auth();

        let bin = self.binary_path.lock().ok().and_then(|p| p.clone());
        let cfg = self.config_path.lock().ok().and_then(|p| p.clone());

        if bin.is_none() || cfg.is_none() {
            self.add_log(format!("⚠️ Simulated auth: {}", svc));
            return Ok(AuthResult {
                success: true,
                message: "Please complete auth in browser (simulation mode)".into(),
                device_code: None,
            });
        }

        let bin = bin.unwrap();
        let cfg = cfg.unwrap();

        let (arg, needs_input) = match svc {
            "claude" => ("-claude-login", false),
            "codex" => ("-codex-login", false),
            "gemini" => ("-login", true), // 需要发送回车确认
            "antigravity" => ("-antigravity-login", false),
            "kimi" => ("-kimi-login", false),
            "copilot" | "kiro" | "kiro-aws" | "kiro-google" | "kiro-github" | "kiro-import"
            | "qwen" => {
                return Err(format!(
                    "{} login is not supported by the bundled CLIProxyAPI backend.",
                    svc
                ))
            }
            _ => return Err(format!("Unknown service: {}", svc)),
        };

        self.add_log(format!("🔐 Starting {} auth...", svc));

        let mut cmd = Command::new(&bin);
        cmd.arg("--config")
            .arg(&cfg)
            .arg(arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Auth spawn failed: {}", e))?;

        let pid = child.id();
        self.add_log(format!("Auth process started (PID: {})", pid));

        // 处理需要输入的情况
        if needs_input {
            let stdin = child.stdin.take();
            let input_data = if svc == "qwen" {
                qwen_email.map(|e| format!("{}\n", e))
            } else if svc == "gemini" {
                Some("\n".to_string()) // 发送回车确认默认项目
            } else {
                None
            };

            if let (Some(mut stdin), Some(data)) = (stdin, input_data) {
                let delay = if svc == "qwen" { 10000 } else { 3000 };
                thread::spawn(move || {
                    thread::sleep(std::time::Duration::from_millis(delay));
                    let _ = stdin.write_all(data.as_bytes());
                    let _ = stdin.flush();
                });
            }
        }

        // 对于 Copilot，尝试捕获设备码
        let mut device_code = None;
        if svc == "copilot" {
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines().take(20).flatten() {
                    if line.contains("enter the code:") {
                        // 提取设备码
                        if let Some(code) = line.split("enter the code:").nth(1) {
                            let code = code.trim().to_string();
                            if !code.is_empty() {
                                device_code = Some(code.clone());
                                self.add_log(format!("📋 Device code: {}", code));
                                // 复制到剪贴板
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(&code);
                                    self.add_log("✓ Code copied to clipboard".into());
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        // 保存认证进程引用，以便后续可以取消
        // 同时启动后台线程等待进程完成
        let child_id = child.id();
        if let Ok(mut auth) = self.auth_process.lock() {
            *auth = Some(child);
        }

        // 启动后台线程，5分钟后自动清理
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_secs(300));
            // 尝试杀死超时的进程
            #[cfg(windows)]
            {
                let mut cmd = Command::new("taskkill");
                cmd.args(["/F", "/PID", &child_id.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                let _ = cmd.status();
            }
            #[cfg(not(windows))]
            let _ = Command::new("kill")
                .args(["-9", &child_id.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            log::info!("Auth process {} timeout cleanup attempted", child_id);
        });

        let message = if svc == "copilot" && device_code.is_some() {
            format!("🌐 Browser opened for GitHub authentication.\n\n📋 Code copied to clipboard:\n\n{}\n\nPaste it in the browser!", device_code.as_ref().unwrap())
        } else {
            "🌐 Browser opened for authentication.\n\nPlease complete the login in your browser."
                .into()
        };

        Ok(AuthResult {
            success: true,
            message,
            device_code,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code: Option<String>,
}

fn same_executable_path(actual: &Path, expected: &Path) -> bool {
    let actual = fs::canonicalize(actual).unwrap_or_else(|_| actual.to_path_buf());
    let expected = fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());

    #[cfg(windows)]
    {
        normalize_windows_path(&actual).eq_ignore_ascii_case(&normalize_windows_path(&expected))
    }

    #[cfg(not(windows))]
    {
        actual == expected
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

#[cfg(target_os = "linux")]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{}/exe", pid)).ok()
}

#[cfg(target_os = "macos")]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    use std::os::raw::c_void;

    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
    }

    let mut buffer = [0_u8; PROC_PIDPATHINFO_MAXSIZE];
    let len = unsafe {
        proc_pidpath(
            pid as i32,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
        )
    };

    if len <= 0 {
        return None;
    }

    Some(PathBuf::from(
        String::from_utf8_lossy(&buffer[..len as usize]).to_string(),
    ))
}

#[cfg(windows)]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    let script = format!(
        "$p = Get-Process -Id {} -ErrorAction SilentlyContinue; if ($p) {{ $p.Path }}",
        pid
    );
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn process_executable_path(_pid: u32) -> Option<PathBuf> {
    None
}

fn terminate_process(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.status();
    }

    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn force_terminate_process(pid: u32) {
    #[cfg(windows)]
    {
        terminate_process(pid);
    }

    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn wait_until_process_exits(pid: u32, timeout: std::time::Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if process_executable_path(pid).is_none() {
            return;
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
}

impl Drop for ProxyServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[cfg(not(windows))]
    #[test]
    fn cleanup_skips_pid_when_executable_path_does_not_match() {
        let test_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_dir = std::env::temp_dir().join(format!(
            "toapiproxy-backend-pid-test-{}-{}",
            std::process::id(),
            test_id
        ));
        fs::create_dir_all(&test_dir).unwrap();

        let expected_backend = test_dir.join("cli-proxy-api");
        fs::write(&expected_backend, "").unwrap();

        let pid_file = test_dir.join("cli-proxy-api.pid");
        let mut external_process = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        fs::write(&pid_file, external_process.id().to_string()).unwrap();

        let server = ProxyServer::new(8317, 8318);
        server.set_pid_file_path(pid_file);
        server.cleanup_recorded_backend_process(&expected_backend);

        thread::sleep(Duration::from_millis(100));
        assert!(external_process.try_wait().unwrap().is_none());

        let _ = external_process.kill();
        let _ = external_process.wait();
        let _ = fs::remove_dir_all(test_dir);
    }
}
