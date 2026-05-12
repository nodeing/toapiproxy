// Tauri API
let invoke;
let invokeBase;
let listen;
const pendingDiagnosticLogs = [];
const diagnosticCommandSkipList = new Set([
    'record_frontend_log',
    'copy_diagnostic_report',
    'open_log_folder',
]);

async function initTauri() {
    // 等待 Tauri API 加载，最多等待 5 秒
    for (let i = 0; i < 50; i++) {
        if (window.__TAURI__) {
            console.log('Tauri object found:', Object.keys(window.__TAURI__));
            let detectedInvoke = null;
            // Tauri 2.x API 结构
            if (window.__TAURI__.core) {
                detectedInvoke = window.__TAURI__.core.invoke;
            } else if (window.__TAURI__.tauri) {
                detectedInvoke = window.__TAURI__.tauri.invoke;
            } else if (window.__TAURI__.invoke) {
                detectedInvoke = window.__TAURI__.invoke;
            }
            
            if (window.__TAURI__.event) {
                listen = window.__TAURI__.event.listen;
            }
            
            if (detectedInvoke) {
                invokeBase = detectedInvoke;
                invoke = createDiagnosticInvoke(detectedInvoke);
                console.log('Tauri API initialized successfully');
                flushDiagnosticLogs();
                return true;
            }
        }
        await new Promise(resolve => setTimeout(resolve, 100));
    }
    console.error('Tauri API not available after 5 seconds');
    return false;
}

// State
let serverRunning = false;
let services = [];
let accounts = [];
let logs = [];
let usageData = {};  // 存储用量数据
let backendUsageSnapshot = null;
let backendUsageTimeRange = 'today';
let backendUsageLoaded = false;
let backendUsageLoading = false;
let backendUsageError = '';
let codexAccounts = [];  // Codex account snapshots
const primaryServiceIds = ['codex'];

// DOM Elements
const navItems = document.querySelectorAll('.nav-item');
const tabContents = document.querySelectorAll('.tab-content');
const toggleServerBtn = document.getElementById('toggle-server');
const serverStatus = document.getElementById('server-status');
const logContainer = document.getElementById('log-container');
const servicesGrid = document.getElementById('services-grid');
const accountsList = document.getElementById('accounts-list');
const codexAccountsList = document.getElementById('codex-accounts-list');
const usageStatisticsContent = document.getElementById('usage-statistics-content');
const confirmModal = document.getElementById('confirm-modal');
const confirmModalTitle = document.getElementById('confirm-modal-title');
const confirmModalMessage = document.getElementById('confirm-modal-message');
const confirmModalCancel = document.getElementById('confirm-modal-cancel');
const confirmModalConfirm = document.getElementById('confirm-modal-confirm');
const confirmModalClose = document.getElementById('confirm-modal-close');

function activatePageTab(group, target) {
    const buttons = document.querySelectorAll(`.page-tab[data-page-group="${group}"]`);
    const panes = document.querySelectorAll(`.page-pane[data-page-group="${group}"]`);
    const actionPanes = document.querySelectorAll(`.page-toolbar-actions-pane[data-page-group="${group}"]`);

    buttons.forEach(button => {
        button.classList.toggle('active', button.dataset.pageTarget === target);
    });

    panes.forEach(pane => {
        pane.classList.toggle('active', pane.dataset.pagePane === target);
    });

    actionPanes.forEach(pane => {
        pane.classList.toggle('active', pane.dataset.pagePaneActions === target);
    });
}

async function handlePageTabActivated(group, target) {
    if (group === 'accounts' && target === 'codex' && typeof loadCodexAccounts === 'function') {
        await loadCodexAccounts();
        return;
    }

    if (group === 'config') {
        if (target === 'claude' && typeof window.loadClaudeProviders === 'function') {
            await window.loadClaudeProviders({ silent: true });
            return;
        }

        if (target === 'codex' && typeof window.loadCodexConfigProfiles === 'function') {
            await window.loadCodexConfigProfiles({ silent: true });
            return;
        }

        if (target === 'droid' && typeof window.loadDroidCustomModels === 'function') {
            await window.loadDroidCustomModels({ silent: true });
        }
    }
}

async function handleMainTabActivated(tabId) {
    if (tabId === 'usage') {
        if (!backendUsageLoaded && !backendUsageLoading) {
            await loadBackendUsageStatistics();
        } else {
            renderBackendUsageStatistics();
        }
    }

    if (tabId === 'accounts') {
        activatePageTab('accounts', 'codex');
        await handlePageTabActivated('accounts', 'codex');
    }

    if (tabId === 'config') {
        activatePageTab('config', 'claude');
        await handlePageTabActivated('config', 'claude');
    }
}

document.querySelectorAll('.page-tab[data-page-group]').forEach(button => {
    button.addEventListener('click', async () => {
        const group = button.dataset.pageGroup;
        const target = button.dataset.pageTarget;
        activatePageTab(group, target);
        await handlePageTabActivated(group, target);
    });
});

// Tab Navigation
async function activateMainSection(tabId) {
    const targetNav = document.querySelector(`[data-tab="${tabId}"]`);
    const targetTab = document.getElementById(tabId);

    navItems.forEach(nav => nav.classList.remove('active'));
    tabContents.forEach(content => content.classList.remove('active'));

    if (targetNav) {
        targetNav.classList.add('active');
    }

    if (targetTab) {
        targetTab.classList.add('active');
    }

    await handleMainTabActivated(tabId);
}

navItems.forEach(item => {
    item.addEventListener('click', async () => {
        await activateMainSection(item.dataset.tab);
        return;

        const tabId = item.dataset.tab;
        navItems.forEach(nav => nav.classList.remove('active'));
        item.classList.add('active');
        tabContents.forEach(content => content.classList.remove('active'));
        document.getElementById(tabId).classList.add('active');

        // 切换到 Codex 页面时刷新 keys
        // Activate nested page tabs when switching main sections.
        await handleMainTabActivated(tabId);
    });
});

// Server Control
toggleServerBtn.addEventListener('click', async () => {
    if (!invoke) {
        addLog('Tauri API 未初始化');
        return;
    }
    
    toggleServerBtn.disabled = true;
    addLog('正在处理...');
    
    try {
        if (serverRunning) {
            const result = await invoke('stop_server');
            serverRunning = false;
            addLog(result);
        } else {
            const result = await invoke('start_server');
            serverRunning = true;
            addLog(result);
        }
        updateServerStatus();
    } catch (error) {
        console.error('Server toggle error:', error);
        addLog(`错误: ${error}`);
    } finally {
        toggleServerBtn.disabled = false;
    }
});

function updateServerStatus() {
    const statusDot = serverStatus.querySelector('.status-dot');
    const statusText = serverStatus.querySelector('.status-text');
    
    if (serverRunning) {
        statusDot.classList.remove('stopped');
        statusDot.classList.add('running');
        statusText.textContent = '运行中';
        toggleServerBtn.textContent = '停止服务器';
        toggleServerBtn.classList.remove('btn-primary');
        toggleServerBtn.classList.add('btn-secondary');
    } else {
        statusDot.classList.remove('running');
        statusDot.classList.add('stopped');
        statusText.textContent = '已停止';
        toggleServerBtn.textContent = '启动服务器';
        toggleServerBtn.classList.remove('btn-secondary');
        toggleServerBtn.classList.add('btn-primary');
    }
}

// Logging
function normalizeClientError(error) {
    if (!error) return '未知错误';
    if (typeof error === 'string') return error;
    if (error instanceof Error) {
        return error.stack || error.message || String(error);
    }
    try {
        return JSON.stringify(error);
    } catch (_) {
        return String(error);
    }
}

function normalizeConsoleValue(value) {
    if (value instanceof Error) {
        return value.stack || value.message || String(value);
    }
    if (typeof value === 'string') {
        return value;
    }
    try {
        return JSON.stringify(value);
    } catch (_) {
        return String(value);
    }
}

function createDiagnosticInvoke(rawInvoke) {
    return async (cmd, args) => {
        try {
            return await rawInvoke(cmd, args);
        } catch (error) {
            if (!diagnosticCommandSkipList.has(cmd)) {
                recordClientDiagnostic(
                    'error',
                    `Tauri command failed: ${cmd}\n${normalizeClientError(error)}`,
                    'tauri.invoke'
                );
            }
            throw error;
        }
    };
}

function recordClientDiagnostic(level, message, context = 'frontend') {
    const payload = {
        level,
        message: String(message || '').slice(0, 6000),
        context,
    };

    if (!invokeBase) {
        pendingDiagnosticLogs.push(payload);
        if (pendingDiagnosticLogs.length > 50) {
            pendingDiagnosticLogs.shift();
        }
        return;
    }

    invokeBase('record_frontend_log', payload).catch(() => {
        pendingDiagnosticLogs.push(payload);
        if (pendingDiagnosticLogs.length > 50) {
            pendingDiagnosticLogs.shift();
        }
    });
}

function flushDiagnosticLogs() {
    if (!invokeBase || pendingDiagnosticLogs.length === 0) {
        return;
    }

    const pending = pendingDiagnosticLogs.splice(0, pendingDiagnosticLogs.length);
    pending.forEach(payload => {
        invokeBase('record_frontend_log', payload).catch(() => {
            pendingDiagnosticLogs.push(payload);
        });
    });
}

function setupClientDiagnostics() {
    if (window.__toapiDiagnosticsReady) {
        return;
    }
    window.__toapiDiagnosticsReady = true;

    window.addEventListener('error', event => {
        const details = [
            event.message || '页面脚本错误',
            event.filename ? `${event.filename}:${event.lineno || 0}:${event.colno || 0}` : '',
            event.error ? normalizeClientError(event.error) : '',
        ].filter(Boolean).join('\n');
        recordClientDiagnostic('error', details, 'window.error');
        addLog(`页面错误: ${event.message || '未知错误'}`);
    });

    window.addEventListener('unhandledrejection', event => {
        const details = normalizeClientError(event.reason);
        recordClientDiagnostic('error', details, 'unhandledrejection');
        addLog(`异步错误: ${String(details).split('\n')[0] || '未知错误'}`);
    });

    const originalError = console.error.bind(console);
    const originalWarn = console.warn.bind(console);

    console.error = (...args) => {
        originalError(...args);
        recordClientDiagnostic('error', args.map(normalizeConsoleValue).join(' '), 'console.error');
    };

    console.warn = (...args) => {
        originalWarn(...args);
        recordClientDiagnostic('warn', args.map(normalizeConsoleValue).join(' '), 'console.warn');
    };
}

function addLog(message) {
    const timestamp = new Date().toLocaleTimeString();
    logs.push(`[${timestamp}] ${message}`);
    if (logs.length > 100) logs = logs.slice(-100);
    renderLogs();
}

function renderLogs() {
    if (logs.length === 0) {
        logContainer.innerHTML = '<div class="log-empty">暂无日志</div>';
        return;
    }
    logContainer.innerHTML = logs.map(log => 
        `<div class="log-line">${escapeHtml(log)}</div>`
    ).join('');
    logContainer.scrollTop = logContainer.scrollHeight;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text ?? '';
    return div.innerHTML;
}

let confirmDialogResolver = null;

function closeConfirmDialog(confirmed) {
    if (!confirmModal) {
        return;
    }

    confirmModal.classList.remove('visible');
    confirmModal.setAttribute('aria-hidden', 'true');
    document.body.classList.remove('body-modal-open');

    const resolver = confirmDialogResolver;
    confirmDialogResolver = null;
    if (resolver) {
        resolver(Boolean(confirmed));
    }
}

window.appConfirm = function(options = {}) {
    if (!confirmModal) {
        return Promise.resolve(window.confirm(options.message || '确定继续吗？'));
    }

    if (confirmDialogResolver) {
        closeConfirmDialog(false);
    }

    confirmModalTitle.textContent = options.title || '确认操作';
    confirmModalMessage.textContent = options.message || '确定继续吗？';
    confirmModalCancel.textContent = options.cancelText || '取消';
    confirmModalConfirm.textContent = options.confirmText || '确认';
    confirmModal.classList.add('visible');
    confirmModal.setAttribute('aria-hidden', 'false');
    document.body.classList.add('body-modal-open');

    return new Promise(resolve => {
        confirmDialogResolver = resolve;
        confirmModalConfirm.focus();
    });
};

if (confirmModal) {
    confirmModalCancel.addEventListener('click', () => closeConfirmDialog(false));
    confirmModalConfirm.addEventListener('click', () => closeConfirmDialog(true));
    confirmModalClose.addEventListener('click', () => closeConfirmDialog(false));
    confirmModal.addEventListener('click', event => {
        if (event.target.closest('[data-confirm-close="cancel"]')) {
            closeConfirmDialog(false);
        }
    });
    document.addEventListener('keydown', event => {
        if (!confirmModal.classList.contains('visible')) {
            return;
        }
        if (event.key === 'Escape') {
            event.preventDefault();
            closeConfirmDialog(false);
        }
        if (event.key === 'Enter') {
            event.preventDefault();
            closeConfirmDialog(true);
        }
    });
}

document.getElementById('clear-logs').addEventListener('click', () => {
    logs = [];
    renderLogs();
    if (invoke) invoke('clear_server_logs');
});

document.getElementById('copy-diagnostic-report')?.addEventListener('click', async event => {
    const button = event.currentTarget;
    const originalText = button.textContent;
    button.disabled = true;
    button.textContent = '复制中...';

    try {
        const result = await invoke('copy_diagnostic_report');
        addLog(`已复制错误报告，共 ${result.lineCount || 0} 行`);
        showNotification('错误报告已复制', result.logDir || '日志信息已复制到剪贴板');
    } catch (error) {
        addLog(`复制错误报告失败: ${error}`);
        alert(`复制错误报告失败: ${error}`);
    } finally {
        button.disabled = false;
        button.textContent = originalText;
    }
});

document.getElementById('open-log-folder')?.addEventListener('click', async () => {
    try {
        await invoke('open_log_folder');
        addLog('已打开日志目录');
    } catch (error) {
        addLog(`打开日志目录失败: ${error}`);
        alert(`打开日志目录失败: ${error}`);
    }
});


// Services
const serviceIcons = {
    claude: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.2 18.6 7v10L12 20.8 5.4 17V7L12 3.2Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path><path d="M12 7.8 15.8 10v4L12 16.2 8.2 14v-4L12 7.8Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>',
    codex: '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="5.5" width="14" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"></rect><path d="M8 18.5h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M10 14.5v4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M14 14.5v4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path></svg>',
    gemini: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5 13.8 8.2 18.5 10 13.8 11.8 12 16.5 10.2 11.8 5.5 10 10.2 8.2Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>',
    copilot: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7.5 15.5c-1.9 0-3.5-1.6-3.5-3.5S5.6 8.5 7.5 8.5c.7-2 2.6-3.3 4.8-3.3 2.8 0 5.2 2.3 5.2 5.2v.3c1.4.3 2.5 1.6 2.5 3.1 0 1.8-1.4 3.2-3.2 3.2H7.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path><path d="M9 12h6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path></svg>',
    qwen: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="1.8"></circle><path d="M16.5 16.5 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M9.5 12.5 11.3 14.3 15 10.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
    kiro: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5 9.5 10H15l-3 10.5L9 13H4.8L12 3.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>',
    antigravity: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 8.5c0-1.9 1.6-3.5 3.5-3.5 1.1 0 2.1.5 2.8 1.4.4-.2.9-.4 1.4-.4 1.7 0 3.1 1.4 3.1 3.1v.3c1.3.4 2.2 1.6 2.2 3 0 1.8-1.4 3.2-3.2 3.2H9.2C7.4 15.6 6 14.2 6 12.4c0-1.7 1.2-3.2 3-3.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
    default: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 12h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M12 8v8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"></circle></svg>'
};

const deleteIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M10 11v6M14 11v6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M6 7l1 14h10l1-14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path><path d="M9 7V4h6v3" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>';

const serviceHelp = {
    'claude': 'Claude AI 服务',
    'codex': 'OpenAI Codex',
    'gemini': 'Google Gemini (使用默认项目)',
    'copilot': 'GitHub Copilot (Claude, GPT, Gemini)',
    'qwen': '通义千问',
    'antigravity': 'Antigravity (Gemini & Claude)'
};

function renderServices() {
    const visibleServices = services.filter(service => primaryServiceIds.includes(service.id));
    servicesGrid.innerHTML = visibleServices.map(service => `
        <div class="service-card" data-service="${service.id}">
            <div class="service-header">
                <span class="service-icon">${serviceIcons[service.id] || serviceIcons.default}</span>
                <span class="service-name">${service.name}</span>
            </div>
            <div class="service-status">
                <span class="status-dot ${service.connected ? 'running' : 'stopped'}"></span>
                <span>${service.connected ? `已连接 (${service.accountCount} 账户)` : '未连接'}</span>
            </div>
            <p class="service-help">${serviceHelp[service.id] || ''}</p>
            ${service.id === 'kiro' ? `
                <div class="kiro-buttons">
                    <button class="btn btn-small" onclick="connectKiro('kiro-google')">Google</button>
                    <button class="btn btn-small" onclick="connectKiro('kiro-github')">GitHub</button>
                    <button class="btn btn-small" onclick="connectKiro('kiro-aws')">AWS</button>
                </div>
                <button class="btn btn-secondary" style="margin-top: 8px; width: 100%;" onclick="importFromKiroIDE()">
                    从 Kiro IDE 导入
                </button>
            ` : service.id === 'qwen' ? `
                <button class="btn ${service.connected ? 'btn-secondary' : 'btn-primary'}" 
                        onclick="connectQwen()">
                    ${service.connected ? '添加账户' : '连接'}
                </button>
            ` : `
                <button class="btn ${service.connected ? 'btn-secondary' : 'btn-primary'}" 
                        onclick="${service.connected ? 'navigateToAccountsManager()' : `toggleService('${service.id}')`}">
                    ${service.connected ? '添加账户' : '连接'}
                </button>
            `}
        </div>
    `).join('');
}

window.navigateToAccountsManager = async function(target = 'codex') {
    await activateMainSection('accounts');
    activatePageTab('accounts', target);
    await handlePageTabActivated('accounts', target);

    const addAccountButton = document.getElementById('add-codex-account');
    addAccountButton?.focus();
    addAccountButton?.scrollIntoView({ behavior: 'smooth', block: 'center' });
};

window.toggleService = async function(serviceId) {
    try {
        addLog(`正在连接 ${serviceId}...`);
        const result = await invoke('connect_service', { serviceId });
        addLog(result.message);
        if (result.device_code) {
            showNotification('设备码已复制', `请在浏览器中输入: ${result.device_code}`);
        }
        setTimeout(refreshState, 2000);
    } catch (error) {
        addLog(`${serviceId} 错误: ${error}`);
    }
};

window.connectKiro = async function(method) {
    try {
        addLog(`正在连接 Kiro (${method})...`);
        const result = await invoke('connect_service', { serviceId: method });
        addLog(result.message);
        setTimeout(refreshState, 2000);
    } catch (error) {
        addLog(`Kiro 错误: ${error}`);
    }
};

window.importFromKiroIDE = async function() {
    try {
        addLog(`正在从 Kiro IDE 导入...`);
        const account = await invoke('import_from_kiro_ide');
        addLog(`已导入账户: ${account.email}`);
        showNotification('导入成功', `已从 Kiro IDE 导入账户`);
        await refreshState();
    } catch (error) {
        addLog(`导入失败: ${error}`);
        alert(`导入失败: ${error}\n\n请确保已在 Kiro IDE 中登录。`);
    }
};

window.connectQwen = async function() {
    const email = prompt('请输入 Qwen 账户邮箱:');
    if (!email) return;
    
    try {
        addLog(`正在连接 Qwen (${email})...`);
        const result = await invoke('connect_service', { serviceId: 'qwen', qwenEmail: email });
        addLog(result.message);
        setTimeout(refreshState, 2000);
    } catch (error) {
        addLog(`Qwen 错误: ${error}`);
    }
};

// Accounts with Usage
function renderAccounts() {
    if (!accountsList) {
        return;
    }

    if (accounts.length === 0) {
        accountsList.innerHTML = `
            <div class="card">
                <div class="card-body" style="text-align: center; padding: 40px;">
                    <p class="account-empty-icon">
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="M16 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path>
                            <circle cx="10" cy="7" r="4" fill="none" stroke="currentColor" stroke-width="1.8"></circle>
                            <path d="M20 21v-2.2a3.5 3.5 0 0 0-2.6-3.4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path>
                        </svg>
                    </p>
                    <p style="color: var(--text-secondary);">暂无账户</p>
                    <p style="color: var(--text-secondary); font-size: 12px; margin-top: 8px;">
                        在"服务"页面连接服务以添加账户
                    </p>
                </div>
            </div>
        `;
        return;
    }
    
    accountsList.innerHTML = accounts.map(account => {
        const usage = account.usage || usageData[account.id];
        const usagePercent = usage ? usage.percent : 0;
        const usageClass = usagePercent >= 90 ? 'danger' : usagePercent >= 70 ? 'warning' : '';
        
        return `
        <div class="account-card">
            <div class="account-header">
                <span class="account-icon">${getProviderIcon(account.provider)}</span>
                <div class="account-info">
                    <div class="account-email">${escapeHtml(account.email)}${account.isExpired ? ' <span class="expired-badge">已过期</span>' : ''}</div>
                    <div class="account-provider">${escapeHtml(account.provider)}${account.subscription ? ` · ${account.subscription}` : ''}</div>
                </div>
                <button class="account-delete" onclick="removeAccount('${account.id}')" title="删除账户">${deleteIcon}</button>
            </div>
            ${usage ? `
                <div class="usage-bar">
                    <div class="usage-track">
                        <div class="usage-fill ${usageClass}" style="width: ${Math.min(usagePercent, 100)}%"></div>
                    </div>
                    <div class="usage-text">
                        <span>${usage.used} / ${usage.limit}${usage.bonusLimit ? ` (+${usage.bonusLimit} bonus)` : ''}</span>
                        <span>${usagePercent}%${usage.resetDays ? ` · ${usage.resetDays}天后重置` : ''}</span>
                    </div>
                </div>
            ` : `
                <div class="usage-placeholder">
                    <span>点击刷新获取用量数据</span>
                </div>
            `}
        </div>
    `}).join('');
}

function getProviderIcon(provider) {
    const icons = {
        'Claude': serviceIcons.claude,
        'Codex': serviceIcons.codex,
        'Gemini': serviceIcons.gemini,
        'GitHub Copilot': serviceIcons.copilot,
        'Qwen': serviceIcons.qwen,
        'Kiro': serviceIcons.kiro,
        'Antigravity': serviceIcons.antigravity,
    };
    return icons[provider] || serviceIcons.default;
}

window.removeAccount = async function(accountId) {
    const confirmed = await window.appConfirm({
        title: '删除账户',
        message: '确定要删除此账户吗？',
        confirmText: '删除'
    });
    if (!confirmed) return;
    try {
        const result = await invoke('remove_account', { accountId });
        addLog(result);
        await refreshState();
    } catch (error) {
        addLog(`删除失败: ${error}`);
    }
};


// Codex Account Management
async function loadCodexAccounts() {
    try {
        const result = await invoke('get_codex_accounts');
        codexAccounts = Array.isArray(result) ? result : [];
        renderCodexAccounts();
    } catch (error) {
        console.error('Failed to load Codex accounts:', error);
        codexAccounts = [];
        renderCodexAccounts();
    }
}

function getUsageLevelClass(percent) {
    if (percent >= 90) return 'danger';
    if (percent >= 70) return 'warning';
    return '';
}

function formatCodexPlan(plan) {
    if (!plan) return '';

    const normalized = String(plan).trim().toLowerCase();
    const labels = {
        guest: 'GUEST',
        free: 'FREE',
        go: 'GO',
        plus: 'PLUS',
        pro: 'PRO',
        team: 'TEAM',
        business: 'BUSINESS',
        education: 'EDU',
        enterprise: 'ENTERPRISE',
    };

    return labels[normalized] || normalized.toUpperCase();
}

function formatCodexCredits(balance, unlimited) {
    if (unlimited) return '额度不限';
    if (balance === null || balance === undefined || Number.isNaN(Number(balance))) return '';
    return `额度 ${Number(balance).toFixed(2)}`;
}

function formatCodexDateTime(value, options = {}) {
    const { includeSeconds = false, fallback = '未知' } = options;
    if (!value) return fallback;

    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return escapeHtml(String(value));

    const datePart = new Intl.DateTimeFormat('zh-CN', {
        year: 'numeric',
        month: 'numeric',
        day: 'numeric',
    }).format(date);
    const timePart = new Intl.DateTimeFormat('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
        ...(includeSeconds ? { second: '2-digit' } : {}),
        hour12: false,
    }).format(date);

    return `${datePart} ${timePart}`;
}

function formatResetSummary(window) {
    if (!window?.resetAt) return '';
    return `重置于 ${formatCodexDateTime(window.resetAt, { includeSeconds: true, fallback: '' })}`;
}

function renderCodexUsageSection(label, window, options = {}) {
    if (!window) return '';

    const usageClass = getUsageLevelClass(window.usedPercent || 0);
    const usedPercent = Math.min(window.usedPercent || 0, 100);
    const remainingPercent = window.remainingPercent ?? Math.max(0, 100 - usedPercent);
    const resetAtText = formatCodexDateTime(window.resetAt, { includeSeconds: true });
    const updatedAtText = options.updatedAt ? `更新于 ${formatCodexDateTime(options.updatedAt)}` : '';
    const resetSummary = formatResetSummary(window);
    const localizedLabel = label === '5h' ? '5 小时限额' : label === 'Weekly' ? '周限额' : label;

    return `
        <div class="codex-usage-section">
            <div class="codex-usage-header">
                <span class="codex-usage-label">${escapeHtml(localizedLabel)}</span>
                <span class="codex-usage-used ${usageClass}">已使用 ${usedPercent}%</span>
            </div>
            <div class="codex-usage-meta">
                <span></span>
                <span>重置于 ${resetAtText}</span>
            </div>
            <div class="usage-track codex-usage-track">
                <div class="usage-fill ${usageClass}" style="width: ${usedPercent}%"></div>
            </div>
            <div class="codex-usage-footer">
                <span>剩余 ${remainingPercent}%</span>
            </div>
            ${updatedAtText || resetSummary ? `
                <div class="codex-usage-extra">
                    <span>${updatedAtText}</span>
                    <span>${resetSummary}</span>
                </div>
            ` : ''}
        </div>
    `;
}

function renderCodexAccounts() {
    if (codexAccounts.length === 0) {
        codexAccountsList.innerHTML = `
            <div class="card">
                <div class="card-body" style="text-align: center; padding: 40px;">
                    <p class="account-empty-icon">${serviceIcons.codex}</p>
                    <p style="color: var(--text-secondary);">暂无 Codex API Key</p>
                    <p style="color: var(--text-secondary); font-size: 12px; margin-top: 8px;">
                        点击"添加账户"输入您的 Codex API Key
                    </p>
                </div>
            </div>
        `;
        return;
    }

    codexAccountsList.innerHTML = codexKeys.map((key, index) => `
        <div class="account-card">
            <div class="account-header">
                <span class="account-icon">${serviceIcons.codex}</span>
                <div class="account-info">
                    <div class="account-email">Codex Key ${index + 1}</div>
                    <div class="account-provider">${key['api-key'] ? '****' + key['api-key'].slice(-4) : 'N/A'}</div>
                </div>
                <button class="account-delete" onclick="deleteCodexKey('${key['api-key']}')" title="删除">${deleteIcon}</button>
            </div>
            ${key['base-url'] ? `<div class="usage-placeholder"><span>Base URL: ${key['base-url']}</span></div>` : ''}
        </div>
    `).join('');
}

window.deleteCodexKey = async function(apiKey) {
    const confirmed = await window.appConfirm({
        title: '删除 Codex API Key',
        message: '确定要删除此 Codex API Key 吗？',
        confirmText: '删除'
    });
    if (!confirmed) return;
    try {
        await invoke('delete_codex_key', { apiKey });
        addLog(`Codex API Key 已删除`);
        await loadCodexKeys();
        await refreshState();
    } catch (error) {
        addLog(`删除失败: ${error}`);
    }
};

// Codex dropdown menu toggle
const codexDropdown = document.getElementById('codex-add-dropdown');
const addCodexBtn = document.getElementById('add-codex-account');

addCodexBtn?.addEventListener('click', (e) => {
    e.stopPropagation();
    const dropdown = document.getElementById('codex-add-dropdown');
    if (dropdown) {
        dropdown.style.display = dropdown.style.display === 'none' ? 'block' : 'none';
    }
});

document.addEventListener('click', () => {
    const dropdown = document.getElementById('codex-add-dropdown');
    if (dropdown) {
        dropdown.style.display = 'none';
    }
});

function scheduleCodexAccountSync() {
    let remainingAttempts = 20;
    const timer = setInterval(async () => {
        remainingAttempts -= 1;
        try {
            if (typeof loadCodexAccounts === 'function') {
                await loadCodexAccounts();
            }
            await refreshState();
        } catch (error) {
            console.warn('Codex sync check failed:', error);
        }

        if (remainingAttempts <= 0) {
            clearInterval(timer);
        }
    }, 2000);
}

// Login with OpenAI - triggers Codex OAuth flow
window.codexLoginOpenAI = async function() {
    const dropdown = document.getElementById('codex-add-dropdown');
    if (dropdown) dropdown.style.display = 'none';

    try {
        addLog('正在启动 Codex OAuth 登录流程...');
        const result = await invoke('connect_service', { serviceId: 'codex' });
        addLog(result.message);
        if (result.device_code) {
            showNotification('设备码已复制', `请在浏览器中输入: ${result.device_code}`);
        }
        scheduleCodexAccountSync();
    } catch (error) {
        addLog(`Codex 登录失败: ${error}`);
    }
};

// Import Current Codex - from local Codex CLI token
window.codexImportCurrent = async function() {
    const dropdown = document.getElementById('codex-add-dropdown');
    if (dropdown) dropdown.style.display = 'none';

    try {
        addLog('正在从本地 Codex CLI 导入 token...');
        const result = await invoke('import_codex_token');
        addLog(result);
        await loadCodexKeys();
        await refreshState();
    } catch (error) {
        addLog(`导入失败: ${error}`);
        alert(`导入失败: ${error}\n\n请确保已安装 Codex CLI 并完成登录。`);
    }
};

// Event listeners for Codex dropdown
document.getElementById('codex-login-openai')?.addEventListener('click', (e) => {
    e.preventDefault();
    window.codexLoginOpenAI();
});

document.getElementById('codex-import-current')?.addEventListener('click', (e) => {
    e.preventDefault();
    window.codexImportCurrent();
});

document.getElementById('refresh-codex-usage')?.addEventListener('click', async () => {
    const btn = document.getElementById('refresh-codex-usage');
    btn.disabled = true;
    btn.textContent = '刷新中...';

    try {
        addLog('正在刷新 Codex API Keys...');
        await loadCodexKeys();
        addLog(`已加载 ${codexKeys.length} 个 Codex API Key`);
    } catch (error) {
        addLog(`刷新失败: ${error}`);
    } finally {
        btn.disabled = false;
        btn.textContent = '刷新用量';
    }
});


// Add account button - navigate to services tab
document.getElementById('add-account')?.addEventListener('click', async () => {
    await window.navigateToAccountsManager('codex');
    addLog('已跳转到账户管理，请使用顶部的添加账户按钮');
    return;
    /*
    addLog('馃挕 宸茶烦杞埌璐︽埛绠＄悊锛岃浣跨敤椤堕儴鐨勬坊鍔犺处鎴峰叆鍙);
    return;

    // 切换到服务页面
    navItems.forEach(nav => nav.classList.remove('active'));
    tabContents.forEach(content => content.classList.remove('active'));
    
    const servicesNav = document.querySelector('[data-tab="services"]');
    const servicesTab = document.getElementById('services');
    
    if (servicesNav) servicesNav.classList.add('active');
    if (servicesTab) servicesTab.classList.add('active');
    
    addLog('请在服务页面选择要连接的服务');
});

    */
});

// Fetch Usage
document.getElementById('refresh-usage')?.addEventListener('click', async () => {
    const btn = document.getElementById('refresh-usage');
    btn.disabled = true;
    btn.textContent = '刷新中...';
    
    try {
        addLog('正在获取用量数据...');
        const results = await invoke('fetch_all_usage');
        
        let successCount = 0;
        for (const result of results) {
            if (result.usage) {
                usageData[result.accountId] = result.usage;
                // 更新账户信息
                const account = accounts.find(a => a.id === result.accountId);
                if (account) {
                    account.usage = result.usage;
                    if (result.subscription) account.subscription = result.subscription;
                    if (result.email && result.email !== 'Unknown') account.email = result.email;
                }
                successCount++;
            } else if (result.error) {
                addLog(`${result.email}: ${result.error}`);
            }
        }
        
        addLog(`已更新 ${successCount} 个账户的用量数据`);
        renderAccounts();
    } catch (error) {
        addLog(`获取用量失败: ${error}`);
    } finally {
        btn.disabled = false;
        btn.textContent = '刷新用量';
    }
});

const backendUsageRangeLabels = {
    today: '今天',
    sevenDays: '7 天',
    thirtyDays: '30 天',
    all: '全部',
};

const backendUsageNumberFormatter = new Intl.NumberFormat('zh-CN');

function getUsageNumericValue(value) {
    const number = Number(value ?? 0);
    return Number.isFinite(number) ? number : 0;
}

function formatUsageNumber(value) {
    return backendUsageNumberFormatter.format(getUsageNumericValue(value));
}

function formatUsageDateTime(value) {
    if (!value) return '未知时间';
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return String(value);

    return new Intl.DateTimeFormat('zh-CN', {
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        hour12: false,
    }).format(date);
}

function formatUsageLatency(value) {
    const latency = getUsageNumericValue(value);
    if (!latency) return '';
    if (latency >= 1000) return `${(latency / 1000).toFixed(1)}s`;
    return `${latency}ms`;
}

function normalizeBackendUsageSnapshot(snapshot) {
    const usage = snapshot?.usage || {};
    return {
        usage: {
            total_requests: getUsageNumericValue(usage.total_requests),
            success_count: getUsageNumericValue(usage.success_count),
            failure_count: getUsageNumericValue(usage.failure_count),
            input_tokens: getUsageNumericValue(usage.input_tokens),
            output_tokens: getUsageNumericValue(usage.output_tokens),
            reasoning_tokens: getUsageNumericValue(usage.reasoning_tokens),
            cached_tokens: getUsageNumericValue(usage.cached_tokens),
            total_tokens: getUsageNumericValue(usage.total_tokens),
            apis: usage.apis || {},
            sources: usage.sources || {},
            requests_by_day: usage.requests_by_day || {},
            requests_by_hour: usage.requests_by_hour || {},
            tokens_by_day: usage.tokens_by_day || {},
            tokens_by_hour: usage.tokens_by_hour || {},
        },
        failed_requests: getUsageNumericValue(snapshot?.failed_requests ?? usage.failure_count),
    };
}

function getUsageEntries(collection) {
    return Object.entries(collection || {}).sort((left, right) => {
        const tokenDiff = getUsageNumericValue(right[1]?.total_tokens) - getUsageNumericValue(left[1]?.total_tokens);
        if (tokenDiff !== 0) return tokenDiff;
        return String(left[0]).localeCompare(String(right[0]));
    });
}

function getRecentUsageDetails(snapshot) {
    const details = [];
    const apis = snapshot?.usage?.apis || {};

    Object.entries(apis).forEach(([apiName, apiUsage]) => {
        Object.entries(apiUsage?.models || {}).forEach(([modelName, modelUsage]) => {
            (modelUsage?.details || []).forEach(detail => {
                details.push({ ...detail, apiName, modelName });
            });
        });
    });

    return details.sort((left, right) => {
        const leftTime = new Date(left.timestamp).getTime();
        const rightTime = new Date(right.timestamp).getTime();
        return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
    });
}

function renderUsageMetric(label, value, meta = '') {
    return `
        <div class="usage-metric-card">
            <div class="usage-metric-label">${escapeHtml(label)}</div>
            <div class="usage-metric-value">${formatUsageNumber(value)}</div>
            ${meta ? `<div class="usage-metric-meta">${escapeHtml(meta)}</div>` : ''}
        </div>
    `;
}

function renderUsageTrend(usage) {
    const dayEntries = Object.entries(usage.tokens_by_day || {}).sort(([left], [right]) => left.localeCompare(right));
    if (!dayEntries.length) return '';

    const maxTokens = Math.max(...dayEntries.map(([, value]) => getUsageNumericValue(value)), 1);
    const visibleEntries = dayEntries.slice(-14);

    return `
        <section class="usage-panel">
            <div class="usage-panel-header">
                <h3>趋势</h3>
                <span>${escapeHtml(backendUsageRangeLabels[backendUsageTimeRange] || '')}</span>
            </div>
            <div class="usage-trend-bars">
                ${visibleEntries.map(([day, tokens]) => {
                    const value = getUsageNumericValue(tokens);
                    const height = Math.max(8, Math.round((value / maxTokens) * 100));
                    return `
                        <div class="usage-trend-item" title="${escapeHtml(day)} · ${formatUsageNumber(value)} tokens">
                            <div class="usage-trend-bar" style="height: ${height}%"></div>
                            <span>${escapeHtml(day.slice(5))}</span>
                        </div>
                    `;
                }).join('')}
            </div>
        </section>
    `;
}

function renderUsageSourceCard(source) {
    const modelEntries = getUsageEntries(source.models).slice(0, 3);
    return `
        <div class="usage-list-item">
            <div class="usage-list-main">
                <div class="usage-list-title">${escapeHtml(source.name || 'unknown source')}</div>
                <div class="usage-list-subtitle">
                    ${escapeHtml(source.provider || 'unknown provider')}${source.auth_index ? ` · auth ${escapeHtml(source.auth_index)}` : ''}
                </div>
            </div>
            <div class="usage-list-stats">
                <strong>${formatUsageNumber(source.total_tokens)}</strong>
                <span>${formatUsageNumber(source.total_requests)} 请求</span>
            </div>
            ${modelEntries.length ? `
                <div class="usage-model-lines">
                    ${modelEntries.map(([modelName, model]) => `
                        <div class="usage-model-line">
                            <span>${escapeHtml(modelName)}</span>
                            <span>${formatUsageNumber(model.total_tokens)} tokens</span>
                        </div>
                    `).join('')}
                </div>
            ` : ''}
        </div>
    `;
}

function renderUsageApiCard(apiName, apiUsage) {
    const modelEntries = getUsageEntries(apiUsage.models).slice(0, 4);
    const modelCount = Object.keys(apiUsage.models || {}).length;
    return `
        <div class="usage-list-item">
            <div class="usage-list-main">
                <div class="usage-list-title">${escapeHtml(apiName)}</div>
                <div class="usage-list-subtitle">${formatUsageNumber(apiUsage.total_requests)} 请求 · ${formatUsageNumber(apiUsage.failure_count)} 失败</div>
            </div>
            <div class="usage-list-stats">
                <strong>${formatUsageNumber(apiUsage.total_tokens)}</strong>
                <span>${formatUsageNumber(modelCount)} 模型</span>
            </div>
            ${modelEntries.length ? `
                <div class="usage-model-lines">
                    ${modelEntries.map(([modelName, model]) => `
                        <div class="usage-model-line">
                            <span>${escapeHtml(modelName)}</span>
                            <span>${formatUsageNumber(model.total_requests)} 次 · ${formatUsageNumber(model.total_tokens)} tokens</span>
                        </div>
                    `).join('')}
                </div>
            ` : ''}
        </div>
    `;
}

function renderUsageRecentRequests(details) {
    if (!details.length) return '';

    return `
        <section class="usage-panel usage-panel-wide">
            <div class="usage-panel-header">
                <h3>最近请求</h3>
                <span>最多显示 12 条</span>
            </div>
            <div class="usage-request-list">
                ${details.slice(0, 12).map(detail => {
                    const tokens = detail.tokens || {};
                    const latency = formatUsageLatency(detail.latency_ms);
                    const statusText = detail.failed ? `失败${detail.fail?.status_code ? ` ${detail.fail.status_code}` : ''}` : '成功';
                    return `
                        <div class="usage-request-row">
                            <div>
                                <div class="usage-request-title">${escapeHtml(detail.modelName || detail.model || 'unknown model')}</div>
                                <div class="usage-request-meta">${escapeHtml(formatUsageDateTime(detail.timestamp))} · ${escapeHtml(detail.source || detail.provider || detail.apiName || 'unknown source')}</div>
                            </div>
                            <div class="usage-request-tokens">
                                <span>${formatUsageNumber(tokens.total_tokens)} tokens</span>
                                <small>in ${formatUsageNumber(tokens.input_tokens)} · out ${formatUsageNumber(tokens.output_tokens)}</small>
                            </div>
                            <div class="usage-request-state ${detail.failed ? 'failed' : ''}">
                                ${escapeHtml(statusText)}${latency ? `<small>${escapeHtml(latency)}</small>` : ''}
                            </div>
                        </div>
                    `;
                }).join('')}
            </div>
        </section>
    `;
}

function renderBackendUsageStatistics() {
    if (!usageStatisticsContent) return;

    document.querySelectorAll('.usage-range-tab').forEach(button => {
        button.classList.toggle('active', button.dataset.usageRange === backendUsageTimeRange);
    });

    if (backendUsageLoading && !backendUsageSnapshot) {
        usageStatisticsContent.innerHTML = `
            <div class="usage-empty-state">
                <span class="loading"></span>
                <span>正在读取用量数据</span>
            </div>
        `;
        return;
    }

    if (!backendUsageSnapshot) {
        usageStatisticsContent.innerHTML = `
            <div class="usage-empty-state">
                ${backendUsageError ? `<strong>${escapeHtml(backendUsageError)}</strong>` : '<span>暂无用量数据</span>'}
            </div>
        `;
        return;
    }

    const usage = backendUsageSnapshot.usage;
    const sourceEntries = getUsageEntries(usage.sources);
    const apiEntries = getUsageEntries(usage.apis);
    const recentDetails = getRecentUsageDetails(backendUsageSnapshot);

    const hasUsage = usage.total_requests > 0 || usage.total_tokens > 0;
    if (!hasUsage) {
        usageStatisticsContent.innerHTML = `
            <div class="usage-empty-state">
                <span>暂无用量数据</span>
                <small>启动服务并产生请求后，这里会显示统计结果。</small>
            </div>
        `;
        return;
    }

    usageStatisticsContent.innerHTML = `
        ${backendUsageError ? `<div class="usage-inline-error">${escapeHtml(backendUsageError)}</div>` : ''}
        <div class="usage-summary-grid">
            ${renderUsageMetric('总 Tokens', usage.total_tokens, `${formatUsageNumber(usage.total_requests)} 次请求`)}
            ${renderUsageMetric('输入', usage.input_tokens)}
            ${renderUsageMetric('输出', usage.output_tokens)}
            ${renderUsageMetric('推理', usage.reasoning_tokens)}
            ${renderUsageMetric('缓存', usage.cached_tokens)}
            ${renderUsageMetric('成功', usage.success_count)}
            ${renderUsageMetric('失败', usage.failure_count)}
            ${renderUsageMetric('来源', sourceEntries.length)}
        </div>
        ${renderUsageTrend(usage)}
        <div class="usage-breakdown-grid">
            <section class="usage-panel">
                <div class="usage-panel-header">
                    <h3>来源</h3>
                    <span>${formatUsageNumber(sourceEntries.length)} 个</span>
                </div>
                <div class="usage-list">
                    ${sourceEntries.slice(0, 6).map(([, source]) => renderUsageSourceCard(source)).join('')}
                </div>
            </section>
            <section class="usage-panel">
                <div class="usage-panel-header">
                    <h3>API</h3>
                    <span>${formatUsageNumber(apiEntries.length)} 个</span>
                </div>
                <div class="usage-list">
                    ${apiEntries.slice(0, 6).map(([apiName, apiUsage]) => renderUsageApiCard(apiName, apiUsage)).join('')}
                </div>
            </section>
        </div>
        ${renderUsageRecentRequests(recentDetails)}
    `;
}

async function loadBackendUsageStatistics() {
    if (!invoke || backendUsageLoading) return;

    const btn = document.getElementById('refresh-usage-statistics');
    backendUsageLoading = true;
    backendUsageError = '';
    if (btn) {
        btn.disabled = true;
        btn.setAttribute('aria-busy', 'true');
    }
    renderBackendUsageStatistics();

    try {
        const result = await invoke('get_backend_usage_statistics', { timeRange: backendUsageTimeRange });
        backendUsageSnapshot = normalizeBackendUsageSnapshot(result);
        backendUsageLoaded = true;
    } catch (error) {
        backendUsageError = `读取失败：${error}`;
        addLog(`用量统计读取失败: ${error}`);
    } finally {
        backendUsageLoading = false;
        if (btn) {
            btn.disabled = false;
            btn.removeAttribute('aria-busy');
        }
        renderBackendUsageStatistics();
    }
}

document.querySelectorAll('.usage-range-tab').forEach(button => {
    button.addEventListener('click', async () => {
        const range = button.dataset.usageRange;
        if (!range || range === backendUsageTimeRange) return;
        backendUsageTimeRange = range;
        backendUsageLoaded = false;
        renderBackendUsageStatistics();
        await loadBackendUsageStatistics();
    });
});

document.getElementById('refresh-usage-statistics')?.addEventListener('click', async () => {
    await loadBackendUsageStatistics();
});

// Open auth folder
document.getElementById('open-auth-folder').addEventListener('click', async () => {
    try {
        await invoke('open_auth_folder');
        addLog('已打开认证文件夹');
    } catch (error) {
        addLog(`打开文件夹失败: ${error}`);
    }
});

// Copy server URL
document.getElementById('copy-url')?.addEventListener('click', async () => {
    try {
        const url = await invoke('copy_server_url');
        addLog(`已复制服务器地址: ${url}`);
        showNotification('已复制', url);
    } catch (error) {
        addLog(`复制失败: ${error}`);
    }
});

// Launch at login toggle - 在 init 中设置
function setupAutostart() {
    const launchAtLoginCheckbox = document.getElementById('launch-at-login');
    if (launchAtLoginCheckbox && invoke) {
        // Load current state
        invoke('get_autostart_enabled').then(enabled => {
            launchAtLoginCheckbox.checked = enabled;
        }).catch(() => {});
        
        launchAtLoginCheckbox.addEventListener('change', async (e) => {
            try {
                await invoke('set_autostart_enabled', { enabled: e.target.checked });
                addLog(e.target.checked ? '已启用开机自启动' : '已禁用开机自启动');
            } catch (error) {
                addLog(`设置失败: ${error}`);
                e.target.checked = !e.target.checked;  // 恢复状态
            }
        });
    }
}

// Notification helper
function showNotification(title, body) {
    try {
        if ('Notification' in window && Notification.permission === 'granted') {
            new Notification(title, { body });
        }
    } catch (_) {
        // Notifications are optional; failures should not interrupt the app.
    }
}

async function setupNotificationPermission() {
    if (!('Notification' in window) || Notification.permission !== 'default') {
        return;
    }

    try {
        await Notification.requestPermission();
    } catch (_) {
        // Notifications are optional; missing permission should not appear as an app error.
    }
}

async function openExternalUrl(url) {
    if (!url) {
        return;
    }

    try {
        if (invoke) {
            await invoke('open_external_url', { url });
            return;
        }

        if (window.__TAURI__?.opener?.openUrl) {
            await window.__TAURI__.opener.openUrl(url);
            return;
        }

        if (window.__TAURI__?.shell?.open) {
            await window.__TAURI__.shell.open(url);
            return;
        }
    } catch (error) {
        console.error('Open external url error:', error);
    }

    window.open(url, '_blank', 'noopener,noreferrer');
}

function setupAboutLinks() {
    document.querySelectorAll('#about-tutorial-link, #about-home-link').forEach(link => {
        link.addEventListener('click', async event => {
            event.preventDefault();
            event.stopPropagation();
            await openExternalUrl(link.href);
        });
    });
}

// Refresh state from backend
async function refreshState() {
    try {
        const state = await invoke('get_app_state');
        serverRunning = state.serverRunning || false;
        accounts = state.accounts || [];
        services = state.services || [];
        
        // 合并服务器日志
        if (state.logs && state.logs.length > 0) {
            for (const log of state.logs) {
                if (!logs.includes(log)) logs.push(log);
            }
            if (logs.length > 100) logs = logs.slice(-100);
            renderLogs();
        }
        
        updateServerStatus();
        renderServices();
        renderAccounts();
    } catch (error) {
        console.error('Refresh state error:', error);
    }
}

// Listen for file changes
async function setupFileWatcher() {
    if (!listen) return;
    
    await listen('auth-files-changed', async () => {
        addLog('检测到认证文件变化，正在刷新...');
        await refreshState();
        if (typeof loadCodexAccounts === 'function') {
            await loadCodexAccounts();
        }
    });
    
    addLog('文件监控已启动');
}

// Initialize
async function init() {
    addLog('正在初始化...');
    
    try {
        setupAboutLinks();
        setupAutostart();
        await setupNotificationPermission();
        await refreshState();
        await setupFileWatcher();
        await loadCodexKeys();
        addLog('应用已启动');
    } catch (error) {
        console.error('Init error:', error);
        addLog('初始化失败，使用默认配置');
        
        services = [
            { id: 'claude', name: 'Claude', connected: false, accountCount: 0 },
            { id: 'codex', name: 'Codex', connected: false, accountCount: 0 },
            { id: 'gemini', name: 'Gemini', connected: false, accountCount: 0 },
            { id: 'copilot', name: 'GitHub Copilot', connected: false, accountCount: 0 },
            { id: 'qwen', name: 'Qwen', connected: false, accountCount: 0 },
            { id: 'antigravity', name: 'Antigravity', connected: false, accountCount: 0 },
        ];
        
        renderServices();
        renderAccounts();
    }
}

// Start app
document.addEventListener('DOMContentLoaded', async () => {
    setupClientDiagnostics();
    const tauriReady = await initTauri();
    if (!tauriReady) {
        console.warn('Tauri not available');
        invokeBase = (cmd, args) => {
            console.log('Mock invoke:', cmd, args);
            return Promise.resolve({ serverRunning: false, accounts: [], services: [], logs: [] });
        };
        invoke = createDiagnosticInvoke(invokeBase);
    }
    
    await init();
    setInterval(refreshState, 30000);  // 每30秒刷新
});
