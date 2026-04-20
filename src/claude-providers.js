(function () {
    const API_FORMAT_OPTIONS = [
        {
            value: 'anthropic-messages',
            label: 'Anthropic Messages（原生）'
        },
        {
            value: 'openai-responses',
            label: 'OpenAI Responses API'
        }
    ];

    const AUTH_FIELD_OPTIONS = [
        { value: 'ANTHROPIC_AUTH_TOKEN', label: 'ANTHROPIC_AUTH_TOKEN（默认）' },
        { value: 'OPENAI_API_KEY', label: 'OPENAI_API_KEY' },
        { value: 'API_KEY', label: 'API_KEY' },
        { value: '__custom__', label: '自定义环境变量' }
    ];

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
        if (!error) {
            return '未知错误';
        }
        if (typeof error === 'string') {
            return error;
        }
        if (error.message) {
            return error.message;
        }
        return String(error);
    }

    function getApiFormatLabel(apiFormat) {
        const matched = API_FORMAT_OPTIONS.find(function (option) {
            return option.value === apiFormat;
        });
        return matched ? matched.label : 'Anthropic Messages（原生）';
    }

    function getListElement() {
        return document.getElementById('claude-providers-list');
    }

    function getNoticeElement() {
        return document.getElementById('claude-providers-notice');
    }

    function hideNotice() {
        const notice = getNoticeElement();
        if (!notice) {
            return;
        }

        notice.style.display = 'none';
        notice.innerHTML = '';
    }

    function showNotice(message) {
        const notice = getNoticeElement();
        if (!notice) {
            return;
        }

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

    function getProfileById(profileId) {
        return profiles.find(function (profile) {
            return profile.id === profileId;
        }) || null;
    }

    function getProfileInitial(name) {
        if (!name) {
            return 'C';
        }
        return String(name).trim().charAt(0).toUpperCase() || 'C';
    }

    function renderBadges(profile) {
        const badges = [];
        badges.push(
            '<span class="provider-badge ' +
                (profile.enabled ? 'current' : 'disabled') +
                '">' +
                escapeHtmlValue(profile.enabled ? '当前生效' : '未启用') +
                '</span>'
        );
        badges.push(
            '<span class="provider-badge format">' +
                escapeHtmlValue(getApiFormatLabel(profile.apiFormat)) +
                '</span>'
        );
        return badges.join('');
    }

    function renderActionButton(action, label, profileId, extraAttributes) {
        const attributes = extraAttributes || '';
        return (
            '<button type="button" class="provider-action-btn" data-provider-action="' +
            action +
            '" data-provider-id="' +
            escapeHtmlValue(profileId) +
            '" ' +
            attributes +
            '>' +
            escapeHtmlValue(label) +
            '</button>'
        );
    }

    function renderProfileCard(profile) {
        const buttons = [];

        buttons.push(
            renderActionButton(
                'toggle-enabled',
                profile.enabled ? '停用' : '启用',
                profile.id,
                'data-provider-enabled="' + (profile.enabled ? 'true' : 'false') + '"'
            )
        );
        buttons.push(renderActionButton('test', '测试', profile.id));
        buttons.push(renderActionButton('copy', '复制', profile.id));
        buttons.push(renderActionButton('edit', '编辑', profile.id));
        buttons.push(renderActionButton('delete', '删除', profile.id));

        return (
            '<div class="provider-card' +
            (profile.enabled ? ' is-current' : ' is-disabled') +
            '">' +
                '<div class="provider-card-main">' +
                    '<div class="provider-avatar">' +
                        escapeHtmlValue(getProfileInitial(profile.name)) +
                    '</div>' +
                    '<div class="provider-meta">' +
                        '<div class="provider-card-topline">' +
                            '<div class="provider-card-title">' +
                                escapeHtmlValue(profile.name) +
                            '</div>' +
                            '<div class="provider-badges">' + renderBadges(profile) + '</div>' +
                        '</div>' +
                        '<div class="provider-card-subtitle">' +
                            escapeHtmlValue(profile.baseUrl || '-') +
                        '</div>' +
                    '</div>' +
                '</div>' +
                '<div class="provider-actions">' + buttons.join('') + '</div>' +
            '</div>'
        );
    }

    function renderProfiles() {
        const list = getListElement();
        if (!list) {
            return;
        }

        if (!profiles.length) {
            list.innerHTML =
                '<div class="provider-empty-state">' +
                    '<div class="provider-empty-title">还没有 Claude 配置档案</div>' +
                    '<div class="provider-empty-copy">新增一个配置档案后，就可以把 Base URL、认证字段、API 密钥和模型映射写入 <code>~/.claude/settings.json</code>。</div>' +
                    '<button class="btn btn-primary" id="provider-empty-add">新增配置档案</button>' +
                '</div>';

            const emptyAddButton = document.getElementById('provider-empty-add');
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
            if (!silent) {
                showNotice('Tauri API 尚未就绪。');
            }
            return;
        }

        try {
            const result = await invoke('get_claude_providers');
            profiles = Array.isArray(result) ? result : [];
            renderProfiles();
            if (!silent) {
                showNotice('');
            }
        } catch (error) {
            console.error('Failed to load Claude profiles:', error);
            profiles = [];
            renderProfiles();
            showNotice('加载 Claude 配置档案失败：' + normalizeError(error));
        }
    }

    function renderAuthFieldOptions() {
        return AUTH_FIELD_OPTIONS.map(function (option) {
            return (
                '<option value="' +
                escapeHtmlValue(option.value) +
                '">' +
                escapeHtmlValue(option.label) +
                '</option>'
            );
        }).join('');
    }

    function renderApiFormatOptions() {
        return API_FORMAT_OPTIONS.map(function (option) {
            return (
                '<option value="' +
                escapeHtmlValue(option.value) +
                '">' +
                escapeHtmlValue(option.label) +
                '</option>'
            );
        }).join('');
    }

    function buildModal() {
        if (modalInitialized) {
            return;
        }

        const wrapper = document.createElement('div');
        wrapper.id = 'claude-provider-modal';
        wrapper.className = 'provider-modal';
        wrapper.innerHTML = [
            '<div class="provider-modal-backdrop" data-provider-modal-close="true"></div>',
            '<div class="provider-modal-panel">',
                '<div class="provider-modal-header">',
                    '<div>',
                        '<div class="provider-modal-title" id="provider-modal-title">新增配置档案</div>',
                    '</div>',
                    '<button type="button" class="btn btn-secondary provider-modal-close" id="provider-modal-close">&times;</button>',
                '</div>',
                '<form class="provider-form" id="claude-provider-form">',
                    '<div class="provider-form-alert info">只有“启用”的配置会写入 <code>~/.claude/settings.json</code>，并且同一时间只会有一个配置生效。</div>',
                    '<div class="provider-form-grid">',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="provider-name">配置名称</label>',
                            '<input class="provider-form-input" id="provider-name" type="text" autocomplete="off" required placeholder="例如：GPT-5.4 本地配置">',
                            '<div class="provider-form-hint">用于区分不同的 Claude / Droid 配置档案。</div>',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="provider-base-url">Base URL</label>',
                            '<input class="provider-form-input" id="provider-base-url" type="text" autocomplete="off" required placeholder="例如：http://127.0.0.1:8317 或 https://api.example.com">',
                            '<div class="provider-form-hint">这里填写 Claude / Droid 最终访问的地址；可以是本地中转，也可以是第三方兼容服务地址。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-api-format">API 格式</label>',
                            '<select class="provider-form-select" id="provider-api-format">' + renderApiFormatOptions() + '</select>',
                            '<div class="provider-form-hint">选择 Base URL 背后实际提供的协议格式。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-auth-field">认证字段</label>',
                            '<select class="provider-form-select" id="provider-auth-field">' + renderAuthFieldOptions() + '</select>',
                            '<div class="provider-form-hint">选择要写入配置的认证环境变量名。</div>',
                        '</div>',
                        '<div class="provider-form-field full" id="provider-custom-auth-field-wrap" style="display:none;">',
                            '<label class="provider-form-label" for="provider-custom-auth-field">自定义认证字段</label>',
                            '<input class="provider-form-input" id="provider-custom-auth-field" type="text" autocomplete="off" placeholder="例如：MINIMAX_API_KEY">',
                            '<div class="provider-form-hint">仅支持大写字母、数字和下划线。</div>',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="provider-api-key">API 密钥</label>',
                            '<div class="provider-form-password">',
                                '<input class="provider-form-input" id="provider-api-key" type="password" autocomplete="off" placeholder="如无认证可留空">',
                                '<button type="button" class="provider-password-toggle" id="provider-api-key-toggle">显示</button>',
                            '</div>',
                            '<div class="provider-form-hint">仅在该配置被启用时写入全局设置；列表里不会明文展示密钥。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-main-model">主模型（ANTHROPIC_MODEL）</label>',
                            '<input class="provider-form-input" id="provider-main-model" type="text" autocomplete="off" required placeholder="gpt-5.4">',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-reasoning-model">推理模型（ANTHROPIC_REASONING_MODEL）</label>',
                            '<input class="provider-form-input" id="provider-reasoning-model" type="text" autocomplete="off" placeholder="gpt-5.4">',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-haiku-model">Haiku 默认模型</label>',
                            '<input class="provider-form-input" id="provider-haiku-model" type="text" autocomplete="off" placeholder="gpt-5.4">',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="provider-sonnet-model">Sonnet 默认模型</label>',
                            '<input class="provider-form-input" id="provider-sonnet-model" type="text" autocomplete="off" placeholder="gpt-5.4">',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="provider-opus-model">Opus 默认模型</label>',
                            '<input class="provider-form-input" id="provider-opus-model" type="text" autocomplete="off" placeholder="gpt-5.4">',
                            '<div class="provider-form-hint">留空时会自动回退到“主模型”。</div>',
                        '</div>',
                    '</div>',
                    '<div class="provider-form-actions">',
                        '<button type="button" class="btn btn-secondary" id="provider-cancel">取消</button>',
                        '<button type="submit" class="btn btn-primary" id="provider-save">保存</button>',
                    '</div>',
                '</form>',
            '</div>'
        ].join('');

        document.body.appendChild(wrapper);
        modalInitialized = true;

        document.getElementById('provider-modal-close').addEventListener('click', closeProfileModal);
        document.getElementById('provider-cancel').addEventListener('click', closeProfileModal);
        document.getElementById('provider-auth-field').addEventListener('change', syncAuthFieldInputState);
        document.getElementById('provider-api-key-toggle').addEventListener('click', toggleApiKeyVisibility);
        wrapper.addEventListener('click', function (event) {
            const closeTrigger = event.target.closest('[data-provider-modal-close="true"]');
            if (closeTrigger) {
                closeProfileModal();
            }
        });
        document.getElementById('claude-provider-form').addEventListener('submit', submitProfileForm);
    }

    function syncAuthFieldInputState() {
        const select = document.getElementById('provider-auth-field');
        const customWrap = document.getElementById('provider-custom-auth-field-wrap');
        if (!select || !customWrap) {
            return;
        }

        const isCustom = select.value === '__custom__';
        customWrap.style.display = isCustom ? 'flex' : 'none';
    }

    function toggleApiKeyVisibility() {
        const input = document.getElementById('provider-api-key');
        const toggle = document.getElementById('provider-api-key-toggle');
        if (!input || !toggle) {
            return;
        }

        const reveal = input.type === 'password';
        input.type = reveal ? 'text' : 'password';
        toggle.textContent = reveal ? '隐藏' : '显示';
    }

    function selectAuthField(authField) {
        const select = document.getElementById('provider-auth-field');
        const customInput = document.getElementById('provider-custom-auth-field');
        if (!select || !customInput) {
            return;
        }

        const matched = AUTH_FIELD_OPTIONS.some(function (option) {
            return option.value === authField && option.value !== '__custom__';
        });

        if (matched) {
            select.value = authField;
            customInput.value = '';
        } else {
            select.value = '__custom__';
            customInput.value = authField || '';
        }

        syncAuthFieldInputState();
    }

    function resolveSelectedAuthField() {
        const select = document.getElementById('provider-auth-field');
        const customInput = document.getElementById('provider-custom-auth-field');
        if (!select) {
            return 'ANTHROPIC_AUTH_TOKEN';
        }
        if (select.value === '__custom__') {
            return customInput ? customInput.value.trim() : '';
        }
        return select.value;
    }

    function openProfileModal(profile) {
        buildModal();

        formState.originalId = profile ? profile.id : null;

        document.getElementById('provider-modal-title').textContent = profile ? '编辑配置档案' : '新增配置档案';
        document.getElementById('provider-name').value = profile ? (profile.name || '') : '';
        document.getElementById('provider-base-url').value = profile ? (profile.baseUrl || '') : 'http://127.0.0.1:8317';
        document.getElementById('provider-api-format').value = profile ? (profile.apiFormat || 'anthropic-messages') : 'openai-responses';
        document.getElementById('provider-api-key').value = profile ? (profile.apiKey || '') : '';
        document.getElementById('provider-api-key').type = 'password';
        document.getElementById('provider-api-key-toggle').textContent = '显示';
        document.getElementById('provider-main-model').value = profile ? (profile.mainModel || '') : '';
        document.getElementById('provider-reasoning-model').value = profile ? (profile.reasoningModel || '') : '';
        document.getElementById('provider-haiku-model').value = profile ? (profile.haikuModel || '') : '';
        document.getElementById('provider-sonnet-model').value = profile ? (profile.sonnetModel || '') : '';
        document.getElementById('provider-opus-model').value = profile ? (profile.opusModel || '') : '';

        selectAuthField(profile ? (profile.authField || 'ANTHROPIC_AUTH_TOKEN') : 'ANTHROPIC_AUTH_TOKEN');
        const modal = document.getElementById('claude-provider-modal');
        if (modal) {
            modal.classList.add('visible');
        }
    }

    function closeProfileModal() {
        const modal = document.getElementById('claude-provider-modal');
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

        const saveButton = document.getElementById('provider-save');
        const previousLabel = saveButton.textContent;
        saveButton.disabled = true;
        saveButton.textContent = '保存中...';

        try {
            const payload = {
                name: document.getElementById('provider-name').value.trim(),
                baseUrl: document.getElementById('provider-base-url').value.trim(),
                apiFormat: document.getElementById('provider-api-format').value,
                authField: resolveSelectedAuthField(),
                apiKey: document.getElementById('provider-api-key').value.trim(),
                mainModel: document.getElementById('provider-main-model').value.trim(),
                reasoningModel: document.getElementById('provider-reasoning-model').value.trim(),
                haikuModel: document.getElementById('provider-haiku-model').value.trim(),
                sonnetModel: document.getElementById('provider-sonnet-model').value.trim(),
                opusModel: document.getElementById('provider-opus-model').value.trim()
            };

            if (!payload.name) {
                throw new Error('配置名称不能为空。');
            }
            if (!payload.baseUrl) {
                throw new Error('Base URL 不能为空。');
            }
            if (!payload.apiFormat) {
                throw new Error('API 格式不能为空。');
            }
            if (!payload.authField) {
                throw new Error('认证字段不能为空。');
            }
            if (!payload.mainModel) {
                throw new Error('主模型不能为空。');
            }

            const result = await invoke('save_claude_provider', {
                provider: payload,
                originalId: formState.originalId
            });

            if (typeof addLog === 'function') {
                addLog(result);
            }

            closeProfileModal();
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to save Claude profile:', error);
            alert(normalizeError(error));
        } finally {
            saveButton.disabled = false;
            saveButton.textContent = previousLabel;
        }
    }

    async function toggleProfileEnabled(profileId, currentlyEnabled) {
        try {
            const result = await invoke('set_claude_provider_enabled', {
                providerId: profileId,
                enabled: !currentlyEnabled
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to toggle Claude profile enabled state:', error);
            alert('更新启用状态失败：' + normalizeError(error));
        }
    }

    async function duplicateProfile(profileId) {
        try {
            const result = await invoke('duplicate_claude_provider', {
                providerId: profileId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to duplicate Claude profile:', error);
            alert('复制配置档案失败：' + normalizeError(error));
        }
    }

    async function testProfile(profileId) {
        try {
            const result = await invoke('test_claude_provider_connectivity', {
                providerId: profileId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            showNotice(result);
        } catch (error) {
            console.error('Failed to test Claude profile:', error);
            alert('连通性测试失败：' + normalizeError(error));
        }
    }

    async function deleteProfile(profileId) {
        const profile = getProfileById(profileId);
        if (!profile) {
            return;
        }

        const confirmed = confirm('确定删除配置档案“' + profile.name + '”吗？');
        if (!confirmed) {
            return;
        }

        try {
            const result = await invoke('delete_claude_provider', {
                providerId: profileId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadProfiles();
            showNotice(result);
        } catch (error) {
            console.error('Failed to delete Claude profile:', error);
            alert('删除配置档案失败：' + normalizeError(error));
        }
    }

    function handleListAction(event) {
        const actionButton = event.target.closest('[data-provider-action]');
        if (!actionButton) {
            return;
        }

        const profileId = actionButton.getAttribute('data-provider-id');
        if (!profileId) {
            return;
        }

        const action = actionButton.getAttribute('data-provider-action');
        if (action === 'toggle-enabled') {
            const currentlyEnabled = actionButton.getAttribute('data-provider-enabled') === 'true';
            toggleProfileEnabled(profileId, currentlyEnabled);
            return;
        }
        if (action === 'test') {
            testProfile(profileId);
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

    function setupPageActions() {
        const list = getListElement();
        if (list) {
            list.addEventListener('click', handleListAction);
        }

        const refreshButton = document.getElementById('refresh-claude-providers');
        if (refreshButton) {
            refreshButton.addEventListener('click', function () {
                loadProfiles();
            });
        }

        const addButton = document.getElementById('add-claude-provider');
        if (addButton) {
            addButton.addEventListener('click', function () {
                openProfileModal();
            });
        }
    }

    window.loadClaudeProviders = loadProfiles;

    document.addEventListener('DOMContentLoaded', function () {
        buildModal();
        setupPageActions();
        setTimeout(function () {
            loadProfiles({ silent: true });
        }, 0);
    });
})();
