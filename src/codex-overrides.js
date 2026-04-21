var codexKeys = [];
var loadCodexKeys;
var loadCodexAccounts;

(function () {
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

    function updateCodexAccountCount() {
        const badge = document.getElementById('codex-account-count');
        if (badge) {
            badge.textContent = String(codexKeys.length);
        }
    }

    function renderCodexCards() {
        if (!codexAccountsList) return;

        updateCodexAccountCount();

        if (!codexKeys.length) {
            codexAccountsList.innerHTML = `
                <div class="card">
                    <div class="card-body codex-empty-state">
                        <p class="codex-empty-icon">&#128187;</p>
                        <p class="codex-empty-title">No Codex accounts yet</p>
                        <p class="codex-empty-copy">
                            Use "Add Account" to import the current Codex login or add a token manually.
                        </p>
                    </div>
                </div>
            `;
            return;
        }

        codexAccountsList.innerHTML = codexKeys.map((account, index) => {
            const planBadge = formatCodexPlan(account.plan);
            const creditsLabel = formatCodexCredits(account.creditsBalance, account.creditsUnlimited);
            const identityParts = [];
            const accountRefArg = JSON.stringify(account.accountRef || account.routeName || account.apiKey);
            const title = account.email || account.displayName || `Codex Key ${index + 1}`;

            if (account.accountId) {
                identityParts.push(
                    `<span class="codex-meta-item codex-account-id">${escapeHtml(account.accountId)}</span>`
                );
            }

            if (creditsLabel) {
                identityParts.push(
                    `<span class="codex-meta-item codex-credits">${escapeHtml(creditsLabel)}</span>`
                );
            }

            const usageMarkup = [
                renderCodexUsageSection('5h', account.primaryWindow),
                renderCodexUsageSection('Weekly', account.secondaryWindow, { updatedAt: account.updatedAt }),
            ].filter(Boolean).join('');

            const placeholder = !usageMarkup ? `
                <div class="usage-placeholder codex-usage-placeholder">
                    <span>${escapeHtml(account.usageError || 'Usage data is not available yet. Click refresh to try again.')}</span>
                </div>
            ` : '';

            return `
                <div class="account-card codex-account-card">
                    <div class="account-header codex-account-header">
                        <span class="account-icon codex-account-icon codex-account-index">${index + 1}</span>
                        <div class="account-info">
                            <div class="codex-title-row">
                                <div class="account-email">${escapeHtml(title)}</div>
                                ${planBadge ? `<span class="codex-plan-badge">${escapeHtml(planBadge)}</span>` : ''}
                            </div>
                            ${identityParts.length ? `<div class="codex-identity-row">${identityParts.join('')}</div>` : ''}
                        </div>
                        <div class="codex-card-actions">
                            <button class="codex-action-btn" onclick="refreshCodexAccounts()" title="Refresh">&#8635;</button>
                            <button class="account-delete" onclick='deleteCodexKey(${accountRefArg})' title="Delete">&#128465;</button>
                        </div>
                    </div>
                    ${usageMarkup ? `<div class="codex-usage-grid">${usageMarkup}</div>` : ''}
                    ${placeholder}
                </div>
            `;
        }).join('');
    }

    async function loadCodexAccountSnapshots() {
        try {
            const snapshots = await invoke('get_codex_accounts');
            codexKeys = snapshots || [];
            if (typeof codexAccounts !== 'undefined') {
                codexAccounts = codexKeys;
            }
            renderCodexCards();
            return codexKeys;
        } catch (error) {
            console.error('Failed to load Codex accounts:', error);
            codexKeys = [];
            if (typeof codexAccounts !== 'undefined') {
                codexAccounts = codexKeys;
            }
            renderCodexCards();
            throw error;
        }
    }

    loadCodexKeys = loadCodexAccountSnapshots;
    loadCodexAccounts = loadCodexAccountSnapshots;
    renderCodexAccounts = renderCodexCards;

    window.refreshCodexAccounts = async function () {
        await loadCodexAccountSnapshots();
    };

    window.deleteCodexKey = async function (accountRef) {
        const confirmed = await window.appConfirm({
            title: '删除 Codex 账户',
            message: '确定要删除这个 Codex 账户吗？',
            confirmText: '删除'
        });
        if (!confirmed) return;
        try {
            await invoke('delete_codex_account', { accountRef });
            addLog('Codex account deleted');
            await loadCodexAccountSnapshots();
            await refreshState();
        } catch (error) {
            addLog(`Delete failed: ${error}`);
        }
    };

    window.codexImportCurrent = async function () {
        const dropdown = document.getElementById('codex-add-dropdown');
        if (dropdown) dropdown.style.display = 'none';

        try {
            addLog('Importing current Codex login...');
            const result = await invoke('import_codex_token');
            addLog(result);
            await loadCodexAccountSnapshots();
            await refreshState();
        } catch (error) {
            addLog(`Import failed: ${error}`);
            alert(`Import failed: ${error}`);
        }
    };

    const refreshButton = document.getElementById('refresh-codex-usage');
    if (refreshButton) {
        const replacement = refreshButton.cloneNode(true);
        refreshButton.replaceWith(replacement);
        replacement.addEventListener('click', async () => {
            replacement.disabled = true;
            replacement.textContent = '刷新中...';

            try {
                addLog('Refreshing Codex usage...');
                await loadCodexAccountSnapshots();
                addLog(`Loaded ${codexKeys.length} Codex account(s)`);
            } catch (error) {
                addLog(`Refresh failed: ${error}`);
            } finally {
                replacement.disabled = false;
                replacement.textContent = '刷新用量';
            }
        });
    }

    setInterval(() => {
        if (codexKeys.length) {
            renderCodexCards();
        }
    }, 30000);
})();
