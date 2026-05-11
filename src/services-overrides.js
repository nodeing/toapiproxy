(function () {
    var serviceOverview = null;
    var codexAccountSnapshots = [];
    var activeServiceTab = 'codex';
    var visibleServiceIds = ['codex'];
    var editingServiceIds = new Set();

    var serviceIcons = {
        claude: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.2 18.6 7v10L12 20.8 5.4 17V7L12 3.2Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path><path d="M12 7.8 15.8 10v4L12 16.2 8.2 14v-4L12 7.8Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>',
        codex: '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="5.5" width="14" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"></rect><path d="M8 18.5h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M10 14.5v4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M14 14.5v4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path></svg>',
        gemini: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5 13.8 8.2 18.5 10 13.8 11.8 12 16.5 10.2 11.8 5.5 10 10.2 8.2Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path></svg>',
        copilot: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7.5 15.5c-1.9 0-3.5-1.6-3.5-3.5S5.6 8.5 7.5 8.5c.7-2 2.6-3.3 4.8-3.3 2.8 0 5.2 2.3 5.2 5.2v.3c1.4.3 2.5 1.6 2.5 3.1 0 1.8-1.4 3.2-3.2 3.2H7.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"></path><path d="M9 12h6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path></svg>',
        qwen: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="1.8"></circle><path d="M16.5 16.5 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M9.5 12.5 11.3 14.3 15 10.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
        antigravity: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 8.5c0-1.9 1.6-3.5 3.5-3.5 1.1 0 2.1.5 2.8 1.4.4-.2.9-.4 1.4-.4 1.7 0 3.1 1.4 3.1 3.1v.3c1.3.4 2.2 1.6 2.2 3 0 1.8-1.4 3.2-3.2 3.2H9.2C7.4 15.6 6 14.2 6 12.4c0-1.7 1.2-3.2 3-3.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
        default: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 12h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><path d="M12 8v8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path><circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"></circle></svg>',
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

    function getPreferredAccount(service) {
        if (!service || !Array.isArray(service.accounts) || !service.preferredAccountName) {
            return null;
        }

        return service.accounts.find(function (account) {
            return account.name === service.preferredAccountName;
        }) || null;
    }

    function getModeLabel(service) {
        return service.mode === 'preferred' && service.preferredAccountName ? '首选账号模式' : '轮询模式';
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

    function renderRoutingDisplay(service) {
        if (!service.accounts || !service.accounts.length) {
            return '<div class="service-account-empty">当前还没有可配置的账号。</div>';
        }

        var preferred = getPreferredAccount(service);
        var preferredLabel = preferred ? buildAccountOptionLabel(preferred) : '未设置';
        var preferredClass = preferred ? '' : ' service-config-value-muted';

        return [
            '<div class="service-routing-panel service-routing-display">',
            '<div class="service-routing-display-header">',
            '<div>',
            '<div class="service-config-label">当前配置</div>',
            '<div class="service-config-title">' + escapeHtmlValue(getModeLabel(service)) + '</div>',
            '</div>',
            '<button class="btn btn-secondary service-edit-btn" onclick="editServiceRouting(\'' + service.id + '\')">编辑设置</button>',
            '</div>',
            '<div class="service-config-grid">',
            '<div class="service-config-item">',
            '<span class="service-config-label">账号模式</span>',
            '<span class="service-config-value">' + escapeHtmlValue(getModeLabel(service)) + '</span>',
            '</div>',
            '<div class="service-config-item">',
            '<span class="service-config-label">首选账号</span>',
            '<span class="service-config-value' + preferredClass + '">' + escapeHtmlValue(preferredLabel) + '</span>',
            '</div>',
            '</div>',
            '<div class="service-summary">' + escapeHtmlValue(renderModeSummary(service)) + '</div>',
            '</div>',
        ].join('');
    }

    function renderRoutingEditor(service) {
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
            '<div class="service-routing-panel service-routing-editor">',
            '<div class="service-routing-display-header">',
            '<div>',
            '<div class="service-config-label">编辑配置</div>',
            '<div class="service-config-title">账号路由设置</div>',
            '</div>',
            '<button class="btn btn-secondary service-edit-btn" onclick="cancelServiceRoutingEdit(\'' + service.id + '\')">取消</button>',
            '</div>',
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
            '<div class="service-summary">修改后需要保存才会生效。</div>',
            '</div>',
        ].join('');
    }

    function renderRoutingControls(service) {
        return editingServiceIds.has(service.id) ? renderRoutingEditor(service) : renderRoutingDisplay(service);
    }

    function renderServiceCard(service) {
        var statusText = service.connected
            ? '已连接 (' + (service.accountCount || 0) + ' 个账号)'
            : '未连接';

        return [
            '<div class="service-card service-card-enhanced" data-service="' + service.id + '">',
            '<div class="service-header">',
            '<span class="service-icon">' + (serviceIcons[service.id] || serviceIcons.default) + '</span>',
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

        var preferredMode = modeSelect.value === 'preferred';
        accountSelect.disabled = !preferredMode;
        if (preferredMode && !accountSelect.value) {
            var firstEnabledOption = Array.from(accountSelect.options).find(function (option) {
                return option.value && !option.disabled;
            });
            if (firstEnabledOption) {
                accountSelect.value = firstEnabledOption.value;
            }
        }
    }

    window.onServiceModeChange = function (serviceId) {
        updateAccountSelectorState(serviceId);
    };

    window.editServiceRouting = function (serviceId) {
        editingServiceIds.add(serviceId);
        renderServiceOverview();

        var modeSelect = document.getElementById('service-mode-' + serviceId);
        if (modeSelect) {
            modeSelect.focus();
        }
    };

    window.cancelServiceRoutingEdit = function (serviceId) {
        editingServiceIds.delete(serviceId);
        renderServiceOverview();
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
            editingServiceIds.delete(serviceId);

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
