(function () {
    const PROVIDER_OPTIONS = [
        { value: 'anthropic', label: 'anthropic' },
        { value: 'openai', label: 'openai' },
        { value: 'generic-chat-completion-api', label: 'generic-chat-completion-api' }
    ];

    let customModels = [];
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
        return document.getElementById('droid-models-list');
    }

    function getNoticeElement() {
        return document.getElementById('droid-models-notice');
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

    function getModelById(modelId) {
        return customModels.find(function (item) {
            return item.id === modelId;
        }) || null;
    }

    function getModelInitial(modelConfig) {
        const source = modelConfig && (modelConfig.displayName || modelConfig.model || modelConfig.provider);
        if (!source) {
            return 'D';
        }
        return String(source).trim().charAt(0).toUpperCase() || 'D';
    }

    function getNextIndex() {
        return customModels.reduce(function (maxValue, item) {
            const indexValue = Number(item.index);
            if (!Number.isFinite(indexValue)) {
                return maxValue;
            }
            return Math.max(maxValue, indexValue);
        }, -1) + 1;
    }

    function normalizeDisplayNameForId(displayName) {
        return String(displayName || '')
            .trim()
            .replace(/\s+/g, '-');
    }

    function buildGeneratedModelId(displayName, indexValue) {
        const normalizedName = normalizeDisplayNameForId(displayName);
        const normalizedIndex = String(indexValue || '').trim();

        if (!normalizedName || normalizedIndex === '') {
            return '';
        }

        return 'custom:' + normalizedName + '-' + normalizedIndex;
    }

    function renderProviderOptions() {
        return PROVIDER_OPTIONS.map(function (option) {
            return (
                '<option value="' +
                escapeHtmlValue(option.value) +
                '">' +
                escapeHtmlValue(option.label) +
                '</option>'
            );
        }).join('');
    }

    function renderBadges(modelConfig) {
        const badges = [];

        if (modelConfig.isCurrent) {
            badges.push('<span class="provider-badge current">当前默认</span>');
        }

        badges.push(
            '<span class="provider-badge format">' +
                escapeHtmlValue((modelConfig.provider || 'unknown').toUpperCase()) +
            '</span>'
        );

        badges.push(
            '<span class="provider-badge enabled">索引 ' +
                escapeHtmlValue(modelConfig.index) +
            '</span>'
        );

        if (modelConfig.noImageSupport) {
            badges.push('<span class="provider-badge disabled">无图像</span>');
        }

        return badges.join('');
    }

    function renderActionButton(action, label, modelId, extraAttributes) {
        const attributes = extraAttributes || '';
        return (
            '<button type="button" class="provider-action-btn" data-droid-action="' +
            action +
            '" data-droid-id="' +
            escapeHtmlValue(modelId) +
            '" ' +
            attributes +
            '>' +
            escapeHtmlValue(label) +
            '</button>'
        );
    }

    function renderModelCard(modelConfig) {
        const buttons = [];
        buttons.push(
            renderActionButton(
                'set-default',
                modelConfig.isCurrent ? '默认中' : '设为默认',
                modelConfig.id,
                modelConfig.isCurrent ? 'disabled aria-disabled="true"' : ''
            )
        );
        buttons.push(renderActionButton('copy', '复制', modelConfig.id));
        buttons.push(renderActionButton('edit', '编辑', modelConfig.id));
        buttons.push(renderActionButton('delete', '删除', modelConfig.id));

        return (
            '<div class="provider-card' + (modelConfig.isCurrent ? ' is-current' : '') + '">' +
                '<div class="provider-card-main">' +
                    '<div class="provider-avatar">' +
                        escapeHtmlValue(getModelInitial(modelConfig)) +
                    '</div>' +
                    '<div class="provider-meta">' +
                        '<div class="provider-card-topline">' +
                            '<div class="provider-card-title">' +
                                escapeHtmlValue(modelConfig.displayName || modelConfig.model || modelConfig.id) +
                            '</div>' +
                            '<div class="provider-badges">' + renderBadges(modelConfig) + '</div>' +
                        '</div>' +
                        '<div class="provider-card-subtitle">' +
                            escapeHtmlValue(modelConfig.baseUrl || '-') +
                        '</div>' +
                    '</div>' +
                '</div>' +
                '<div class="provider-actions">' + buttons.join('') + '</div>' +
            '</div>'
        );
    }

    function renderModels() {
        const list = getListElement();
        if (!list) {
            return;
        }

        if (!customModels.length) {
            list.innerHTML =
                '<div class="provider-empty-state">' +
                    '<div class="provider-empty-title">还没有 Droid 自定义模型</div>' +
                    '<div class="provider-empty-copy">新增后会写入 <code>~/.factory/settings.json</code> 的 <code>customModels</code> 数组，用于给 Droid 提供自定义模型入口。</div>' +
                    '<button class="btn btn-primary" id="droid-empty-add">新增 Droid 自定义模型</button>' +
                '</div>';

            const emptyAddButton = document.getElementById('droid-empty-add');
            if (emptyAddButton) {
                emptyAddButton.addEventListener('click', function () {
                    openModelModal();
                });
            }
            return;
        }

        list.innerHTML = customModels.map(renderModelCard).join('');
    }

    async function loadCustomModels(options) {
        const silent = options && options.silent;
        const ready = await ensureInvokeReady();
        if (!ready) {
            if (!silent) {
                showNotice('Tauri API 尚未就绪。');
            }
            return;
        }

        try {
            const result = await invoke('get_droid_custom_models');
            customModels = Array.isArray(result) ? result : [];
            renderModels();
            if (!silent) {
                showNotice('');
            }
        } catch (error) {
            console.error('Failed to load Droid custom models:', error);
            customModels = [];
            renderModels();
            showNotice('加载 Droid 自定义模型失败：' + normalizeError(error));
        }
    }

    function buildModal() {
        if (modalInitialized) {
            return;
        }

        const wrapper = document.createElement('div');
        wrapper.id = 'droid-model-modal';
        wrapper.className = 'provider-modal';
        wrapper.innerHTML = [
            '<div class="provider-modal-backdrop" data-droid-modal-close="true"></div>',
            '<div class="provider-modal-panel">',
                '<div class="provider-modal-header">',
                    '<div>',
                        '<div class="provider-modal-title" id="droid-model-modal-title">新增 Droid 自定义模型</div>',
                    '</div>',
                    '<button type="button" class="btn btn-secondary provider-modal-close" id="droid-model-modal-close">&times;</button>',
                '</div>',
                '<form class="provider-form" id="droid-model-form">',
                    '<div class="provider-form-alert info">保存后会更新 <code>~/.factory/settings.json</code> 中的 <code>customModels</code> 数组；如果当前默认模型正好是它，修改 ID 时会同步更新默认模型引用。</div>',
                    '<div class="provider-form-grid">',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="droid-model-display-name">显示名称</label>',
                            '<input class="provider-form-input" id="droid-model-display-name" type="text" autocomplete="off" required placeholder="例如：CC: Opus 4.5 (High)">',
                            '<div class="provider-form-hint">用于在 Droid 的模型列表中显示。</div>',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="droid-model-name">模型名</label>',
                            '<input class="provider-form-input" id="droid-model-name" type="text" autocomplete="off" required placeholder="例如：claude-opus-4-5-20251101-thinking-32000">',
                            '<div class="provider-form-hint">写入 <code>customModels[].model</code>。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="droid-model-id">模型 ID</label>',
                            '<input class="provider-form-input" id="droid-model-id" type="text" autocomplete="off" required placeholder="例如：custom:CC:-Opus-4.5-(High)-0">',
                            '<div class="provider-form-hint">新增时会根据“显示名称 + 索引”自动生成，用于 Droid 内部引用。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="droid-model-index">索引</label>',
                            '<input class="provider-form-input" id="droid-model-index" type="number" min="0" step="1" required>',
                            '<div class="provider-form-hint">新增时会自动填入下一个索引。</div>',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-form-label" for="droid-model-base-url">Base URL</label>',
                            '<input class="provider-form-input" id="droid-model-base-url" type="text" autocomplete="off" required placeholder="例如：http://127.0.0.1:8317 或 http://127.0.0.1:8317/v1">',
                            '<div class="provider-form-hint">写入 <code>customModels[].baseUrl</code>。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="droid-model-provider">Provider</label>',
                            '<select class="provider-form-select" id="droid-model-provider">' + renderProviderOptions() + '</select>',
                            '<div class="provider-form-hint">支持 <code>anthropic</code>、<code>openai</code>、<code>generic-chat-completion-api</code>。</div>',
                        '</div>',
                        '<div class="provider-form-field">',
                            '<label class="provider-form-label" for="droid-model-api-key">API Key</label>',
                            '<div class="provider-form-password">',
                                '<input class="provider-form-input" id="droid-model-api-key" type="password" autocomplete="off" placeholder="默认：dummy-not-used">',
                                '<button type="button" class="provider-password-toggle" id="droid-model-api-key-toggle">显示</button>',
                            '</div>',
                            '<div class="provider-form-hint">默认会写入 <code>dummy-not-used</code>。</div>',
                        '</div>',
                        '<div class="provider-form-field full">',
                            '<label class="provider-checkbox" for="droid-model-no-image-support">',
                                '<input id="droid-model-no-image-support" type="checkbox">',
                                '<span>禁用图像支持（写入 <code>noImageSupport: true</code>）</span>',
                            '</label>',
                        '</div>',
                    '</div>',
                    '<div class="provider-form-actions">',
                        '<button type="button" class="btn btn-secondary" id="droid-model-cancel">取消</button>',
                        '<button type="submit" class="btn btn-primary" id="droid-model-save">保存</button>',
                    '</div>',
                '</form>',
            '</div>'
        ].join('');

        document.body.appendChild(wrapper);
        modalInitialized = true;

        document.getElementById('droid-model-modal-close').addEventListener('click', closeModelModal);
        document.getElementById('droid-model-cancel').addEventListener('click', closeModelModal);
        document.getElementById('droid-model-api-key-toggle').addEventListener('click', toggleApiKeyVisibility);
        document.getElementById('droid-model-display-name').addEventListener('input', syncGeneratedFieldsForCreate);
        document.getElementById('droid-model-index').addEventListener('input', syncGeneratedFieldsForCreate);
        wrapper.addEventListener('click', function (event) {
            const closeTrigger = event.target.closest('[data-droid-modal-close="true"]');
            if (closeTrigger) {
                closeModelModal();
            }
        });
        document.getElementById('droid-model-form').addEventListener('submit', submitModelForm);
    }

    function toggleApiKeyVisibility() {
        const input = document.getElementById('droid-model-api-key');
        const toggle = document.getElementById('droid-model-api-key-toggle');
        if (!input || !toggle) {
            return;
        }

        const reveal = input.type === 'password';
        input.type = reveal ? 'text' : 'password';
        toggle.textContent = reveal ? '隐藏' : '显示';
    }

    function syncGeneratedFieldsForCreate() {
        if (formState.originalId) {
            return;
        }

        const displayNameInput = document.getElementById('droid-model-display-name');
        const indexInput = document.getElementById('droid-model-index');
        const idInput = document.getElementById('droid-model-id');

        if (!displayNameInput || !indexInput || !idInput) {
            return;
        }

        idInput.value = buildGeneratedModelId(displayNameInput.value, indexInput.value);
    }

    function syncGeneratedFieldState(isCreateMode) {
        const idInput = document.getElementById('droid-model-id');
        const indexInput = document.getElementById('droid-model-index');

        if (!idInput || !indexInput) {
            return;
        }

        idInput.readOnly = isCreateMode;
        if (isCreateMode) {
            syncGeneratedFieldsForCreate();
        }
    }

    function openModelModal(modelConfig) {
        buildModal();

        formState.originalId = modelConfig ? modelConfig.id : null;

        document.getElementById('droid-model-modal-title').textContent = modelConfig ? '编辑 Droid 自定义模型' : '新增 Droid 自定义模型';
        document.getElementById('droid-model-display-name').value = modelConfig ? (modelConfig.displayName || '') : '';
        document.getElementById('droid-model-name').value = modelConfig ? (modelConfig.model || '') : '';
        document.getElementById('droid-model-id').value = modelConfig ? (modelConfig.id || '') : '';
        document.getElementById('droid-model-index').value = modelConfig ? String(modelConfig.index) : String(getNextIndex());
        document.getElementById('droid-model-base-url').value = modelConfig ? (modelConfig.baseUrl || '') : 'http://127.0.0.1:8317';
        document.getElementById('droid-model-provider').value = modelConfig ? (modelConfig.provider || 'anthropic') : 'anthropic';
        document.getElementById('droid-model-api-key').value = modelConfig ? (modelConfig.apiKey || '') : 'dummy-not-used';
        document.getElementById('droid-model-api-key').type = 'password';
        document.getElementById('droid-model-api-key-toggle').textContent = '显示';
        document.getElementById('droid-model-no-image-support').checked = Boolean(modelConfig && modelConfig.noImageSupport);
        syncGeneratedFieldState(!modelConfig);

        const modal = document.getElementById('droid-model-modal');
        if (modal) {
            modal.classList.add('visible');
        }
    }

    function closeModelModal() {
        const modal = document.getElementById('droid-model-modal');
        if (modal) {
            modal.classList.remove('visible');
        }
    }

    async function submitModelForm(event) {
        event.preventDefault();

        const ready = await ensureInvokeReady();
        if (!ready) {
            alert('Tauri API 尚未就绪。');
            return;
        }

        const saveButton = document.getElementById('droid-model-save');
        const previousLabel = saveButton.textContent;
        saveButton.disabled = true;
        saveButton.textContent = '保存中...';

        try {
            const rawIndex = document.getElementById('droid-model-index').value.trim();
            const parsedIndex = Number(rawIndex);

            if (!Number.isInteger(parsedIndex) || parsedIndex < 0) {
                throw new Error('索引必须是大于等于 0 的整数。');
            }

            const payload = {
                displayName: document.getElementById('droid-model-display-name').value.trim(),
                model: document.getElementById('droid-model-name').value.trim(),
                id: document.getElementById('droid-model-id').value.trim(),
                index: parsedIndex,
                baseUrl: document.getElementById('droid-model-base-url').value.trim(),
                provider: document.getElementById('droid-model-provider').value.trim(),
                apiKey: document.getElementById('droid-model-api-key').value.trim(),
                noImageSupport: document.getElementById('droid-model-no-image-support').checked
            };

            if (!payload.displayName) {
                throw new Error('显示名称不能为空。');
            }
            if (!payload.model) {
                throw new Error('模型名不能为空。');
            }
            if (!payload.id) {
                throw new Error('模型 ID 不能为空。');
            }
            if (!payload.baseUrl) {
                throw new Error('Base URL 不能为空。');
            }
            if (!payload.provider) {
                throw new Error('Provider 不能为空。');
            }

            const result = await invoke('save_droid_custom_model', {
                modelConfig: payload,
                originalId: formState.originalId
            });

            if (typeof addLog === 'function') {
                addLog(result);
            }

            closeModelModal();
            await loadCustomModels();
            showNotice(result);
        } catch (error) {
            console.error('Failed to save Droid custom model:', error);
            alert(normalizeError(error));
        } finally {
            saveButton.disabled = false;
            saveButton.textContent = previousLabel;
        }
    }

    async function duplicateModel(modelId) {
        try {
            const result = await invoke('duplicate_droid_custom_model', {
                modelId: modelId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadCustomModels();
            showNotice(result);
        } catch (error) {
            console.error('Failed to duplicate Droid custom model:', error);
            alert('复制 Droid 自定义模型失败：' + normalizeError(error));
        }
    }

    async function setDefaultModel(modelId) {
        try {
            const result = await invoke('set_droid_default_model', {
                modelId: modelId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadCustomModels();
            showNotice(result);
        } catch (error) {
            console.error('Failed to set Droid default model:', error);
            alert('设置默认模型失败：' + normalizeError(error));
        }
    }

    async function deleteModel(modelId) {
        const modelConfig = getModelById(modelId);
        if (!modelConfig) {
            return;
        }

        const confirmed = confirm(
            '确定删除 Droid 自定义模型“' +
            modelConfig.displayName +
            '”吗？' +
            (modelConfig.isCurrent ? ' 这是当前默认模型，删除后会清空默认模型引用。' : '')
        );
        if (!confirmed) {
            return;
        }

        try {
            const result = await invoke('delete_droid_custom_model', {
                modelId: modelId
            });
            if (typeof addLog === 'function') {
                addLog(result);
            }
            await loadCustomModels();
            showNotice(result);
        } catch (error) {
            console.error('Failed to delete Droid custom model:', error);
            alert('删除 Droid 自定义模型失败：' + normalizeError(error));
        }
    }

    function handleListAction(event) {
        const actionButton = event.target.closest('[data-droid-action]');
        if (!actionButton) {
            return;
        }

        const modelId = actionButton.getAttribute('data-droid-id');
        if (!modelId) {
            return;
        }

        const action = actionButton.getAttribute('data-droid-action');
        if (action === 'set-default') {
            if (!actionButton.disabled) {
                setDefaultModel(modelId);
            }
            return;
        }
        if (action === 'copy') {
            duplicateModel(modelId);
            return;
        }
        if (action === 'edit') {
            openModelModal(getModelById(modelId));
            return;
        }
        if (action === 'delete') {
            deleteModel(modelId);
        }
    }

    function setupPageActions() {
        const list = getListElement();
        if (list) {
            list.addEventListener('click', handleListAction);
        }

        const refreshButton = document.getElementById('refresh-droid-models');
        if (refreshButton) {
            refreshButton.addEventListener('click', function () {
                loadCustomModels();
            });
        }

        const addButton = document.getElementById('add-droid-model');
        if (addButton) {
            addButton.addEventListener('click', function () {
                openModelModal();
            });
        }
    }

    window.loadDroidCustomModels = loadCustomModels;

    document.addEventListener('DOMContentLoaded', function () {
        buildModal();
        setupPageActions();
        setTimeout(function () {
            loadCustomModels({ silent: true });
        }, 0);
    });
})();
