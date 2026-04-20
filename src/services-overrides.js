(function () {
    var serviceOverview = null;
    var codexAccountSnapshots = [];
    var activeServiceTab = 'codex';
    var visibleServiceIds = ['codex'];

    var serviceIcons = {
        claude: '&#129302;',
        codex: '&#128187;',
        gemini: '&#10024;',
        copilot: '&#128640;',
        qwen: '&#127760;',
        antigravity: '&#127756;',
    };

    var serviceHelp = {
        claude: 'Claude AI',
        codex: 'OpenAI Codex',
        gemini: 'Google Gemini',
        copilot: 'GitHub Copilot',
        qwen: 'Qwen',
        antigravity: 'Antigravity',
    };

    function escapeHtmlValue(text) {
        var div = document.createElement('div');
        div.textContent = text == null ? '' : String(text);
        return div.innerHTML;
    }

    function addLogLine(message) {
        if (typeof addLog === 'function') {
            addLog(message);
        }
    }

    function getServicesGrid() {
        return document.getElementById('services-grid');
    }

    function getServicesSection() {
        return document.getElementById('services');
    }

    function getServicesTabsMount() {
        return document.getElementById('services-tabs');
    }

    function persistActiveServiceTab() {
        try {
            window.localStorage.setItem('toapiproxy-service-tab', activeServiceTab);
        } catch (error) {
            console.warn('Failed to persist service tab:', error);
        }
    }

    function restoreActiveServiceTab() {
        try {
            var saved = window.localStorage.getItem('toapiproxy-service-tab');
            if (saved && visibleServiceIds.indexOf(saved) !== -1) {
                activeServiceTab = saved;
            }
        } catch (error) {
            console.warn('Failed to restore service tab:', error);
        }
    }

    function ensureTabsMount() {
        var mount = getServicesTabsMount();
        if (mount) {
            return mount;
        }

        var section = getServicesSection();
        var grid = getServicesGrid();
        if (!section || !grid) {
            return null;
        }

        mount = document.createElement('div');
        mount.id = 'services-tabs';
        mount.className = 'services-tabs';
        section.insertBefore(mount, grid);
        return mount;
    }

    function getVisibleServices() {
        if (!serviceOverview || !Array.isArray(serviceOverview.services)) {
            return [];
        }

        var visibleServices = serviceOverview.services.filter(function (service) {
            return visibleServiceIds.indexOf(service.id) !== -1;
        });

        if (activeServiceTab === 'all') {
            return visibleServices;
        }

        return visibleServices.filter(function (service) {
            return service.id === activeServiceTab;
        });
    }

    function renderServiceTabs() {
        var mount = ensureTabsMount();
        if (!mount || !serviceOverview || !Array.isArray(serviceOverview.services)) {
            return;
        }

        var tabs = [{ id: 'all', label: '全部' }].concat(
            serviceOverview.services.filter(function (service) {
                return visibleServiceIds.indexOf(service.id) !== -1;
            }).map(function (service) {
                return {
                    id: service.id,
                    label: service.name,
                };
            })
        );

        var validIds = tabs.map(function (tab) { return tab.id; });
        tabs[0].label = '\u5168\u90e8';
        if (validIds.indexOf(activeServiceTab) === -1) {
            activeServiceTab = 'codex';
        }

        mount.innerHTML = tabs.map(function (tab) {
            var activeClass = tab.id === activeServiceTab ? ' active' : '';
            return (
                '<button class="service-tab' + activeClass + '" onclick="switchServiceTab(\'' + tab.id + '\')">' +
                escapeHtmlValue(tab.label) +
                '</button>'
            );
        }).join('');

        if (mount.children.length > 1 && mount.firstElementChild) {
            mount.removeChild(mount.firstElementChild);
        }
    }

    function buildAccountOptionLabel(account) {
        var suffixes = [];
        var label = account.email || account.displayName || account.name || 'Unnamed account';

        if (account.plan) {
            suffixes.push(String(account.plan).toUpperCase());
        }

        if (account.maskedKey) {
            suffixes.push(account.maskedKey);
        }

        if (account.disabled) {
            suffixes.push('已停用');
        } else if (account.unavailable) {
            suffixes.push('暂不可用');
        }

        if (Number(account.priority || 0) > 0) {
            suffixes.push('优先级 ' + account.priority);
        }

        if (!suffixes.length) {
            return label;
        }

        return label + ' (' + suffixes.join(' / ') + ')';
    }

    function mergeCodexSnapshotsIntoOverview(overview, snapshots) {
        if (!overview || !Array.isArray(overview.services)) {
            return overview;
        }

        var snapshotByRouteName = new Map();
        (Array.isArray(snapshots) ? snapshots : []).forEach(function (snapshot) {
            if (snapshot && snapshot.routeName) {
                snapshotByRouteName.set(snapshot.routeName, snapshot);
            }
        });

        overview.services.forEach(function (service) {
            if (service.id !== 'codex' || !Array.isArray(service.accounts)) {
                return;
            }

            if (snapshotByRouteName.size > 0) {
                service.accounts = service.accounts.filter(function (account) {
                    return account && typeof account.name === 'string' && snapshotByRouteName.has(account.name);
                });
            }

            service.accounts.forEach(function (account) {
                if (!account || typeof account.name !== 'string') {
                    return;
                }

                var snapshot = snapshotByRouteName.get(account.name);
                if (!snapshot && account.name.indexOf('codex-key::') === 0) {
                    var index = Number(account.name.split('::')[1]);
                    if (Number.isInteger(index) && index >= 0) {
                        snapshot = snapshots[index];
                    }
                }
                if (!snapshot) {
                    return;
                }

                if (snapshot.email) {
                    account.email = snapshot.email;
                    account.displayName = snapshot.email;
                } else if (snapshot.displayName) {
                    account.displayName = snapshot.displayName;
                }

                if (snapshot.plan) {
                    account.plan = snapshot.plan;
                }

                if (snapshot.maskedApiKey) {
                    account.maskedKey = snapshot.maskedApiKey;
                }

                if (snapshot.accountId && !account.account) {
                    account.account = snapshot.accountId;
                }
            });

            service.accountCount = service.accounts.length;
            service.connected = service.accounts.length > 0;
            if (
                service.preferredAccountName &&
                !service.accounts.some(function (account) { return account.name === service.preferredAccountName; })
            ) {
                service.preferredAccountName = null;
                service.mode = 'round-robin';
            }
        });

        return overview;
    }

    function renderModeSummary(service) {
        if (!service.accounts || !service.accounts.length) {
            return '先连接至少一个账号，再配置轮询或首选账号模式。';
        }

        if (service.mode === 'preferred' && service.preferredAccountName) {
            var preferred = service.accounts.find(function (account) {
                return account.name === service.preferredAccountName;
            });
            var preferredLabel = preferred ? buildAccountOptionLabel(preferred) : service.preferredAccountName;
            return '当前为首选账号模式，优先使用 ' + preferredLabel + '，用完后自动轮询其他账号。';
        }

        return '当前为轮询模式，所有可用账号会按同优先级参与轮询。';
    }

    function renderConnectionActions(service) {
        if (service.id === 'kiro') {
            return [
                '<div class="kiro-buttons">',
                '<button class="btn btn-small" onclick="connectKiro(\'kiro-google\')">Google</button>',
                '<button class="btn btn-small" onclick="connectKiro(\'kiro-github\')">GitHub</button>',
                '<button class="btn btn-small" onclick="connectKiro(\'kiro-aws\')">AWS</button>',
                '</div>',
                '<button class="btn btn-secondary service-connect-btn" onclick="importFromKiroIDE()">导入 Kiro IDE</button>',
            ].join('');
        }

        var buttonLabel = service.connected ? '新增账号' : '连接';
        var buttonClass = service.connected ? 'btn-secondary' : 'btn-primary';

        if (service.id === 'qwen') {
            return '<button class="btn ' + buttonClass + ' service-connect-btn" onclick="connectQwen()">' + buttonLabel + '</button>';
        }

        var actionHandler = service.connected
            ? 'navigateToAccountsManager()'
            : 'toggleService(\'' + service.id + '\')';

        return '<button class="btn ' + buttonClass + ' service-connect-btn" onclick="' + actionHandler + '">' + buttonLabel + '</button>';
    }

    function renderRoutingControls(service) {
        if (!service.accounts || !service.accounts.length) {
            return '<div class="service-account-empty">当前还没有可配置的账号。</div>';
        }

        var mode = service.mode === 'preferred' ? 'preferred' : 'round-robin';
        var preferredName = service.preferredAccountName || '';
        var selectDisabled = mode !== 'preferred';

        var options = service.accounts.map(function (account) {
            var selected = account.name === preferredName ? ' selected' : '';
            var disabled = account.disabled ? ' disabled' : '';
            return (
                '<option value="' + escapeHtmlValue(account.name) + '"' + selected + disabled + '>' +
                escapeHtmlValue(buildAccountOptionLabel(account)) +
                '</option>'
            );
        }).join('');

        return [
            '<div class="service-routing-panel">',
            '<div class="service-field">',
            '<label for="service-mode-' + service.id + '">账号模式</label>',
            '<select id="service-mode-' + service.id + '" class="service-select" onchange="onServiceModeChange(\'' + service.id + '\')">',
            '<option value="round-robin"' + (mode === 'round-robin' ? ' selected' : '') + '>轮询模式</option>',
            '<option value="preferred"' + (mode === 'preferred' ? ' selected' : '') + '>首选账号模式</option>',
            '</select>',
            '</div>',
            '<div class="service-field">',
            '<label for="service-account-' + service.id + '">首选账号</label>',
            '<select id="service-account-' + service.id + '" class="service-select"' + (selectDisabled ? ' disabled' : '') + '>',
            '<option value="">请选择账号</option>',
            options,
            '</select>',
            '</div>',
            '<div class="service-routing-actions">',
            '<button class="btn btn-primary" id="service-apply-' + service.id + '" onclick="applyServiceAccountMode(\'' + service.id + '\')">保存设置</button>',
            '</div>',
            '<div class="service-summary">' + escapeHtmlValue(renderModeSummary(service)) + '</div>',
            '</div>',
        ].join('');
    }

    function renderServiceCard(service) {
        var statusText = service.connected
            ? '已连接 (' + (service.accountCount || 0) + ' 个账号)'
            : '未连接';

        return [
            '<div class="service-card service-card-enhanced" data-service="' + service.id + '">',
            '<div class="service-header">',
            '<span class="service-icon">' + (serviceIcons[service.id] || '&#128274;') + '</span>',
            '<div class="service-header-copy">',
            '<span class="service-name">' + escapeHtmlValue(service.name) + '</span>',
            '<p class="service-help">' + escapeHtmlValue(serviceHelp[service.id] || '') + '</p>',
            '</div>',
            '</div>',
            '<div class="service-meta-row">',
            '<div class="service-status">',
            '<span class="status-dot ' + (service.connected ? 'running' : 'stopped') + '"></span>',
            '<span>' + escapeHtmlValue(statusText) + '</span>',
            '</div>',
            '<span class="service-account-count">账号数: ' + (service.accountCount || 0) + '</span>',
            '</div>',
            renderRoutingControls(service),
            renderConnectionActions(service),
            '</div>',
        ].join('');
    }

    function renderServiceOverview() {
        var grid = getServicesGrid();
        if (!grid || !serviceOverview || !Array.isArray(serviceOverview.services)) {
            return;
        }

        renderServiceTabs();

        var notice = '';
        if (serviceOverview.globalStrategy && serviceOverview.globalStrategy !== 'round-robin') {
            notice =
                '<div class="service-routing-notice">' +
                '当前全局 routing.strategy = ' + escapeHtmlValue(serviceOverview.globalStrategy) +
                '。保存这里的设置时，应用会自动恢复到 round-robin，以确保“首选账号先用、其余账号继续轮询”。' +
                '</div>';
        }

        var visibleServices = getVisibleServices();
        grid.classList.toggle('services-grid-single', true);
        grid.innerHTML = notice + visibleServices.map(renderServiceCard).join('');
    }

    async function loadServiceRoutingOverview() {
        if (typeof invoke !== 'function') {
            return null;
        }

        try {
            codexAccountSnapshots = await invoke('get_codex_accounts').catch(function () {
                return [];
            });
            serviceOverview = await invoke('get_service_routing_overview');
            mergeCodexSnapshotsIntoOverview(serviceOverview, codexAccountSnapshots);
            renderServiceOverview();
            return serviceOverview;
        } catch (error) {
            console.error('Failed to load service routing overview:', error);
            return null;
        }
    }

    function updateAccountSelectorState(serviceId) {
        var modeSelect = document.getElementById('service-mode-' + serviceId);
        var accountSelect = document.getElementById('service-account-' + serviceId);
        if (!modeSelect || !accountSelect) {
            return;
        }

        accountSelect.disabled = modeSelect.value !== 'preferred';
    }

    window.onServiceModeChange = function (serviceId) {
        updateAccountSelectorState(serviceId);
    };

    window.applyServiceAccountMode = async function (serviceId) {
        if (typeof invoke !== 'function') {
            return;
        }

        var modeSelect = document.getElementById('service-mode-' + serviceId);
        var accountSelect = document.getElementById('service-account-' + serviceId);
        var applyButton = document.getElementById('service-apply-' + serviceId);

        if (!modeSelect || !applyButton) {
            return;
        }

        var mode = modeSelect.value;
        var preferredAccountName = accountSelect ? accountSelect.value : '';

        if (mode === 'preferred' && !preferredAccountName) {
            alert('请选择一个首选账号。');
            return;
        }

        applyButton.disabled = true;
        var previousText = applyButton.textContent;
        applyButton.textContent = '保存中...';

        try {
            var result = await invoke('apply_service_account_mode', {
                serviceId: serviceId,
                mode: mode,
                preferredAccountName: mode === 'preferred' ? preferredAccountName : null,
            });
            addLogLine(result);

            if (typeof window.refreshState === 'function') {
                await window.refreshState();
            } else {
                await loadServiceRoutingOverview();
            }
        } catch (error) {
            console.error('Failed to apply service account mode:', error);
            addLogLine('服务模式保存失败: ' + error);
            alert('保存失败: ' + error);
        } finally {
            applyButton.disabled = false;
            applyButton.textContent = previousText;
        }
    };

    window.refreshServicesRouting = loadServiceRoutingOverview;
    window.switchServiceTab = function (serviceId) {
        activeServiceTab = serviceId || 'codex';
        persistActiveServiceTab();
        renderServiceOverview();
    };

    var originalRefreshState = typeof window.refreshState === 'function' ? window.refreshState : null;
    if (originalRefreshState) {
        window.refreshState = async function () {
            var result = await originalRefreshState.apply(this, arguments);
            await loadServiceRoutingOverview();
            return result;
        };
    }

    document.addEventListener('DOMContentLoaded', function () {
        restoreActiveServiceTab();
        setTimeout(function () {
            loadServiceRoutingOverview();
        }, 0);
    });
})();
