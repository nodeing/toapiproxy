use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
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

        self.kill_orphans();
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

        let mut child = cmd.spawn().map_err(|e| format!("Spawn failed: {}", e))?;

        let pid = child.id();
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

    fn kill_orphans(&self) {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/F", "/IM", "cli-proxy-api-plus.exe"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.status();
        }

        #[cfg(not(windows))]
        let _ = Command::new("pkill")
            .args(["-9", "-f", "cli-proxy-api-plus"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
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
            "copilot" => ("-github-copilot-login", false),
            "gemini" => ("-login", true), // 需要发送回车确认
            "kiro" | "kiro-aws" => ("-kiro-aws-login", false),
            "kiro-google" => ("-kiro-google-login", false),
            "kiro-github" => ("-kiro-github-login", false),
            "kiro-import" => ("-kiro-import", false),
            "qwen" => ("-qwen-login", true), // 需要输入邮箱
            "antigravity" => ("-antigravity-login", false),
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

impl Drop for ProxyServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
