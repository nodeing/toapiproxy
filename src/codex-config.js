(function () {
    const REASONING_OPTIONS = ['minimal', 'low', 'medium', 'high', 'xhigh'];
    const WIRE_API_OPTIONS = ['responses'];
    const DEFAULT_PROFILE = {
        name: '',
        providerId: 'toapiproxy',
        providerName: 'ToapiProxy',
        apiKey: 'dummy-not-used',
        baseUrl: 'http://127.0.0.1:8317/v1',
        model: 'gpt-5.4',
        reasoningEffort: 'xhigh',
        wireApi: 'responses',
        requiresOpenAIAuth: true,
        disableResponseStorage: true
    };

    let profiles = [];
    let modalInitialized = false;
    const formState = {
        originalId: null
    };

    function escapeHtmlValue(text) {
        const div = document.createElement('div');
        div.textContent = text == null ? '' : String(text);
        return div.innerHTML;
    }

    function normalizeError(error) {
        if (!error) return '未知错误';
        if (typeof error === 'string') return error;
        if (error.message) return error.message;
        return String(error);
    }

    async function ensureInvokeReady() {
        for (let index = 0; index < 50; index += 1) {
            if (typeof invoke === 'function') {
                return true;
            }
            await new Promise(function (resolve) {
                setTimeout(resolve, 100);
            });
        }
        return false;
    }

    function getListElement() {
        return document.getElementById('codex-configs-list');
    }

    function getNoticeElement() {
        return document.getElementById('codex-configs-notice');
    }

    function hideNotice() {
        const notice = getNoticeElement();
        if (!notice) return;
        notice.style.display = 'none';
        notice.innerHTML = '';
    }

    function showNotice(message) {
        const notice = getNoticeElement();
        if (!notice) return;
        if (!message) {
            hideNotice();
            return;
        }

        notice.innerHTML =
            '<div class="provider-inline-notice__message">' +
            escapeHtmlValue(message) +
            '</div>' +
            '<button type="button" class="provider-inline-notice__close" aria-label="关闭提示">&times;</button>';
        notice.style.display = 'block';

        const closeButton = notice.querySelector('.provider-inline-notice__close');
        if (closeButton) {
            closeButton.addEventListener('click', hideNotice, { once: true });
        }
    }

    function getProfileById(profileId) {
        return profiles.find(function (profile) {
            return profile.id === profileId;
        }) || null;
    }

    function renderOptions(options) {
        return options.map(function (option) {
            return (
                '<option value="' +
                escapeHtmlValue(option) +
                '">' +
                escapeHtmlValue(option) +
                '</option>'
            );
        }).join('');
    }

    function renderBadges(profile) {
        const badges = [];
        if (profile.isCurrent || profile.enabled) {
            badges.push('<span class="provider-badge current">当前生效</span>');
        }
        badges.push(
            '<span class="provider-badge format">' +
            escapeHtmlValue((profile.wireApi || 'responses').toUpperCase()) +
            '</span>'
        );
        badges.push(
            '<span class="provider-badge enabled">' +
            escapeHtmlValue(profile.reasoningEffort || 'xhigh') +
            '</span>'
        );
        return badges.join('');
    }

    function renderActionButton(action, label, profileId) {
        return (
            '<button type="button" class="provider-action-btn" data-codex-config-action="' +
            action +
            '" data-codex-config-id="' +
            escapeHtmlValue(profileId) +
            '">' +
            escapeHtmlValue(label) +
            '</button>'
        );
    }

    function renderProfileCard(profile, index) {
        const buttons = [
            renderActionButton('apply', profile.isCurrent ? '重新应用' : '应用', profile.id),
            renderActionButton('copy', '复制', profile.id),
            renderActionButton('edit', '编辑', profile.id),
            renderActionButton('delete', '删除', profile.id)
        ];

        return (
            '<div class="provider-card' + (profile.isCurrent ? ' is-current' : '') + '">' +
            '<div class="provider-card-main">' +
            '<div class="provider-avatar provider-index-avatar">' +
            escapeHtmlValue(index + 1) +
            '</div>' +
            '<div class="provider-meta">' +
            '<div class="provider-card-topline">' +
            '<div class="provider-card-title">' +
            escapeHtmlValue(profile.name) +
            '</div>' +
            '<div class="provider-badges">' +
            renderBadges(profile) +
            '</div>' +
            '</div>' +
            '<div class="provider-card-subtitle">' +
            escapeHtmlValue(profile.baseUrl || '-') +
            '</div>' +
            '<div class="provider-card-details">' +
            '<div class="provider-card-detail"><strong>model_provider</strong><span>' +
            escapeHtmlValue(profile.providerId || '-') +
            '</span></div>' +
            '<div class="provider-card-detail"><strong>provider name</strong><span>' +
            escapeHtmlValue(profile.providerName || '-') +
            '</span></div>' +
            '<div class="provider-card-detail"><strong>model</strong><span>' +
            escapeHtmlValue(profile.model || '-') +
            '</span></div>' +
            '<div class="provider-card-detail"><strong>OPENAI_API_KEY</strong><span>' +
            (profile.apiKey ? '已配置' : '未设置') +
            '</span></div>' +
            '<div class="provider-card-detail"><strong>requires_openai_auth</strong><span>' +
            (profile.requiresOpenAIAuth ? 'true' : 'false') +
            '</span></div>' +
            '<div class="provider-card-detail"><strong>disable_response_storage</strong><span>' +
            (profile.disableResponseStorage ? 'true' : 'false') +
            '</span></div>' +
            '</div>' +
            '</div>' +
            '</div>' +
            '<div class="provider-actions">' +
            buttons.join('') +
            '</div>' +
            '</div>'
        );
    }

    function renderProfiles() {
        const list = getListElement();
        if (!list) return;

        if (!profiles.length) {
            list.innerHTML =
                '<div class="provider-empty-state">' +
                '<div class="provider-empty-title">还没有 Codex 配置档案</div>' +
                '<div class="provider-empty-copy">新增配置档案后，可以把 model_provider、Base URL、模型和 OPENAI_API_KEY 写入 <code>~/.codex/config.toml</code> 与 <code>~/.codex/auth.json</code>。</div>' +
                '<button class="btn btn-primary" id="codex-config-empty-add">新增配置档案</button>' +
                '</div>';

            const emptyAddButton = document.getElementById('codex-config-empty-add');
            if (emptyAddButton) {
                emptyAddButton.addEventListener('click', function () {
                    openProfileModal();
                });
            }
            return;
        }

        list.innerHTML = profiles.map(renderProfileCard).join('');
    }

    async function loadProfiles(options) {
        const silent = options && options.silent;
        const ready = await ensureInvokeReady();
        if (!ready) {
            if (!silent) showNotice('Tauri API 尚未就绪。');
            return;
        }

        try {
            const result = await invoke('get_codex_config_profiles');
            profiles = Array.isArray(result) ? result : [];
            renderProfiles();
            if (!silent) showNotice('');
        } catch (error) {
            console.error('Failed to load Codex config profiles:', error);
            profiles = [];
            renderProfiles();
            showNotice('加载 Codex 配置档案失败：' + normalizeError(error));
        }
    }

    function buildModal() {
        if (modalInitialized) return;

        const wrapper = document.createElement('div');
        wrapper.id = 'codex-config-modal';
        wrapper.className = 'provider-modal';
        wrapper.innerHTML = [
            '<div class="provider-modal-backdrop" data-codex-config-modal-close="true"></div>',
            '<div class="provider-modal-panel">',
            '<div class="provider-modal-header">',
            '<div>',
            '<div class="provider-modal-title" id="codex-config-modal-title">新增 Codex 配置档案</div>',
            '</div>',
            '<button type="button" class="btn btn-secondary provider-modal-close" id="codex-config-modal-close">&times;</button>',
            '</div>',
            '<form class="provider-form" id="codex-config-form">',
            '<div class="provider-form-alert info">点击“应用”后会写入 <code>~/.codex/config.toml</code> 和 <code>~/.codex/auth.json</code>。</div>',
            '<div class="provider-form-grid">',
            '<div class="provider-form-field full">',
            '<label class="provider-form-label" for="codex-config-name">配置名称</label>',
            '<input class="provider-form-input" id="codex-config-name" type="text" autocomplete="off" required placeholder="例如：Codex 本地代理">',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-form-label" for="codex-config-provider-id">Provider ID</label>',
            '<input class="provider-form-input" id="codex-config-provider-id" type="text" autocomplete="off" required placeholder="toapiproxy">',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-form-label" for="codex-config-provider-name">Provider Name</label>',
            '<input class="provider-form-input" id="codex-config-provider-name" type="text" autocomplete="off" placeholder="ToapiProxy">',
            '</div>',
            '<div class="provider-form-field full">',
            '<label class="provider-form-label" for="codex-config-base-url">Base URL</label>',
            '<input class="provider-form-input" id="codex-config-base-url" type="text" autocomplete="off" required placeholder="http://127.0.0.1:8317/v1">',
            '<div class="provider-form-hint">Codex CLI 的 OpenAI Responses 兼容接口地址。</div>',
            '</div>',
            '<div class="provider-form-field full">',
            '<label class="provider-form-label" for="codex-config-api-key">OPENAI_API_KEY</label>',
            '<div class="provider-form-password">',
            '<input class="provider-form-input" id="codex-config-api-key" type="password" autocomplete="off" placeholder="dummy-not-used">',
            '<button type="button" class="provider-password-toggle" id="codex-config-api-key-toggle">显示</button>',
            '</div>',
            '<div class="provider-form-hint">本地代理可使用 dummy-not-used；如果目标服务需要鉴权，则填写对应密钥。</div>',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-form-label" for="codex-config-model">Model</label>',
            '<input class="provider-form-input" id="codex-config-model" type="text" autocomplete="off" required placeholder="gpt-5.4">',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-form-label" for="codex-config-reasoning">Reasoning Effort</label>',
            '<select class="provider-form-select" id="codex-config-reasoning">' + renderOptions(REASONING_OPTIONS) + '</select>',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-form-label" for="codex-config-wire-api">Wire API</label>',
            '<select class="provider-form-select" id="codex-config-wire-api">' + renderOptions(WIRE_API_OPTIONS) + '</select>',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-checkbox">',
            '<input id="codex-config-requires-openai-auth" type="checkbox">',
            '<span>requires_openai_auth</span>',
            '</label>',
            '</div>',
            '<div class="provider-form-field">',
            '<label class="provider-checkbox">',
            '<input id="codex-config-disable-response-storage" type="checkbox">',
            '<span>disable_response_storage</span>',
            '</label>',
            '</div>',
            '</div>',
            '<div class="provider-form-actions">',
            '<button type="button" class="btn btn-secondary" id="codex-config-defaults">本地代理默认值</button>',
            '<span style="flex:1"></span>',
            '<button type="button" class="btn btn-secondary" id="codex-config-cancel">取消</button>',
            '<button type="submit" class="btn btn-primary" id="codex-config-save">保存</button>',
            '</div>',
            '</form>',
            '</div>'
        ].join('');

        document.body.appendChild(wrapper);
        modalInitialized = true;

        document.getElementById('codex-config-modal-close').addEventListener('click', closeProfileModal);
        document.getElementById('codex-config-cancel').addEventListener('click', closeProfileModal);
        document.getElementById('codex-config-defaults').addEventListener('click', function () {
            fillForm(DEFAULT_PROFILE);
        });
        document.getElementById('codex-config-api-key-toggle').addEventListener('click', toggleApiKeyVisibility);
        wrapper.addEventListener('click', function (event) {
            if (event.target.closest('[data-codex-config-modal-close="true"]')) {
                closeProfileModal();
            }
        });
        document.getElementById('codex-config-form').addEventListener('submit', submitProfileForm);
    }

    function toggleApiKeyVisibility() {
        const input = document.getElementById('codex-config-api-key');
        const toggle = document.getElementById('codex-config-api-key-toggle');
        if (!input || !toggle) return;

        const reveal = input.type === 'password';
        input.type = reveal ? 'text' : 'password';
        toggle.textContent = reveal ? '隐藏' : '显示';
    }

    function fillForm(profile) {
        document.getElementById('codex-config-name').value = profile.name || '';
        document.getElementById('codex-config-provider-id').value = profile.providerId || DEFAULT_PROFILE.providerId;
        document.getElementById('codex-config-provider-name').value = profile.providerName || DEFAULT_PROFILE.providerName;
        document.getElementById('codex-config-base-url').value = profile.baseUrl || DEFAULT_PROFILE.baseUrl;
        document.getElementById('codex-config-api-key').value = profile.apiKey || DEFAULT_PROFILE.apiKey;
        document.getElementById('codex-config-model').value = profile.model || DEFAULT_PROFILE.model;
        document.getElementById('codex-config-reasoning').value = profile.reasoningEffort || DEFAULT_PROFILE.reasoningEffort;
        document.getElementById('codex-config-wire-api').value = profile.wireApi || DEFAULT_PROFILE.wireApi;
        document.getElementById('codex-config-requires-openai-auth').checked = profile.requiresOpenAIAuth !== false;
        document.getElementById('codex-config-disable-response-storage').checked = profile.disableResponseStorage !== false;
    }

    function openProfileModal(profile) {
        buildModal();
        formState.originalId = profile ? profile.id : null;
        document.getElementById('codex-config-modal-title').textContent = profile ? '编辑 Codex 配置档案' : '新增 Codex 配置档案';
        fillForm(profile || { ...DEFAULT_PROFILE, name: 'Codex 本地代理' });
        document.getElementById('codex-config-api-key').type = 'password';
        document.getElementById('codex-config-api-key-toggle').textContent = '显示';

        const modal = document.getElementById('codex-config-modal');
        if (modal) {
            modal.classList.add('visible');
        }
    }

    function closeProfileModal() {
        const modal = document.getElementById('codex-config-modal');
        if (modal) {
            modal.classList.remove('visible');
        }
    }

    async function submitProfileForm(event) {
        event.preventDefault();
        const ready = await ensureInvokeReady();
        if (!ready) {
            alert('Tauri API 尚未就绪。');
            return;
        }

        const saveButton = document.getElementById('codex-config-save');
        const previousLabel = saveButton.textContent;
        saveButton.disabled = true;
        saveButton.textContent = '保存中...';

        try {
            const payload = {
                name: document.getElementById('codex-config-name').value.trim(),
                providerId: document.getElementById('codex-config-provider-id').value.trim(),
                providerName: document.getElementById('codex-config-provider-name').value.trim(),
                baseUrl: document.getElementById('codex-config-base-url').value.trim(),
                apiKey: document.getElementById('codex-config-api-key').value.trim(),
                model: document.getElementById('codex-config-model').value.trim(),
                reasoningEffort: document.getElementById('codex-config-reasoning').value,
                wireApi: document.getElementById('codex-config-wire-api').value,
                requiresOpenAIAuth: document.getElementById('codex-config-requires-openai-auth').checked,
                disableResponseStorage: document.getElementById('codex-config-disable-response-storage').checked
            };

            if (!payload.name) throw new Error('配置名称不能为空。');
            if (!payload.providerId) throw new Error('Provider ID 不能为空。');
            if (!payload.baseUrl) throw new Error('Base URL 不能为空。');
            if (!payload.model) throw new Error('Model 不能为空。');

            const result = await invoke('save_codex_config_profile', {
                profile: payload,
                originalId: formState.originalId
            });

            if (typeof addLog === 'function') addLog(result);
            closeProfileModal();
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to save Codex config profile:', error);
            alert(normalizeError(error));
        } finally {
            saveButton.disabled = false;
            saveButton.textContent = previousLabel;
        }
    }

    async function applyProfile(profileId) {
        try {
            const result = await invoke('apply_codex_config_profile', { profileId });
            if (typeof addLog === 'function') addLog(result);
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to apply Codex config profile:', error);
            alert('应用 Codex 配置失败：' + normalizeError(error));
        }
    }

    async function duplicateProfile(profileId) {
        try {
            const result = await invoke('duplicate_codex_config_profile', { profileId });
            if (typeof addLog === 'function') addLog(result);
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to duplicate Codex config profile:', error);
            alert('复制 Codex 配置失败：' + normalizeError(error));
        }
    }

    async function deleteProfile(profileId) {
        const profile = getProfileById(profileId);
        if (!profile) return;

        const confirmed = await window.appConfirm({
            title: '删除 Codex 配置档案',
            message: '确定删除配置档案“' + profile.name + '”吗？',
            confirmText: '删除'
        });
        if (!confirmed) return;

        try {
            const result = await invoke('delete_codex_config_profile', { profileId });
            if (typeof addLog === 'function') addLog(result);
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to delete Codex config profile:', error);
            alert('删除 Codex 配置失败：' + normalizeError(error));
        }
    }

    function handleListAction(event) {
        const actionButton = event.target.closest('[data-codex-config-action]');
        if (!actionButton) return;

        const profileId = actionButton.getAttribute('data-codex-config-id');
        if (!profileId) return;

        const action = actionButton.getAttribute('data-codex-config-action');
        if (action === 'apply') {
            applyProfile(profileId);
            return;
        }
        if (action === 'copy') {
            duplicateProfile(profileId);
            return;
        }
        if (action === 'edit') {
            openProfileModal(getProfileById(profileId));
            return;
        }
        if (action === 'delete') {
            deleteProfile(profileId);
        }
    }

    document.getElementById('codex-configs-list')?.addEventListener('click', handleListAction);
    document.getElementById('add-codex-config')?.addEventListener('click', function () {
        openProfileModal();
    });
    document.getElementById('refresh-codex-configs')?.addEventListener('click', async function () {
        const button = document.getElementById('refresh-codex-configs');
        const previousLabel = button.textContent;
        button.disabled = true;
        button.textContent = '刷新中...';
        try {
            await loadProfiles();
        } finally {
            button.disabled = false;
            button.textContent = previousLabel;
        }
    });

    window.loadCodexConfigProfiles = loadProfiles;
})();
