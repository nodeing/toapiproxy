use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub struct AuthFileWatcher {
    watcher: Option<notify_debouncer_mini::Debouncer<RecommendedWatcher>>,
    running: Arc<Mutex<bool>>,
}

impl AuthFileWatcher {
    pub fn new() -> Self {
        Self {
            watcher: None,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 启动文件监控
    pub fn start(&mut self, app_handle: AppHandle) -> Result<(), String> {
        let auth_dir = Self::get_auth_dir();

        // 确保目录存在
        if !auth_dir.exists() {
            std::fs::create_dir_all(&auth_dir).map_err(|e| e.to_string())?;
        }

        let (tx, rx) = channel();

        // 创建防抖动的监控器 (500ms 防抖)
        let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
            .map_err(|e| format!("Failed to create watcher: {}", e))?;

        // 监控认证目录
        debouncer
            .watcher()
            .watch(&auth_dir, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch directory: {}", e))?;

        *self.running.lock().unwrap() = true;
        let running = self.running.clone();

        // 在后台线程处理文件变化事件
        thread::spawn(move || {
            log::info!("File watcher started for: {:?}", auth_dir);

            while *running.lock().unwrap() {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(events)) => {
                        for event in events {
                            if let DebouncedEventKind::Any = event.kind {
                                let path = &event.path;
                                if path.extension().map(|e| e == "json").unwrap_or(false) {
                                    log::info!("Auth file changed: {:?}", path);
                                    // 发送事件到前端
                                    let _ = app_handle.emit("auth-files-changed", ());
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        log::error!("Watch error: {:?}", e);
                    }
                    Err(_) => {
                        // 超时，继续循环
                    }
                }
            }
            log::info!("File watcher stopped");
        });

        self.watcher = Some(debouncer);
        Ok(())
    }

    /// 停止文件监控
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        self.watcher = None;
    }

    /// 获取认证目录路径
    pub fn get_auth_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".cli-proxy-api")
    }
}

impl Drop for AuthFileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
