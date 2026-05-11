# 协作规则

## 提交前安全检查

- 每次提交前必须运行 `make security-check`。
- 只有 `make security-check` 通过后，才可以创建提交。
- 检查失败时，先移除个人 API Key、token、认证文件、本地配置文件或包含个人账号信息的内容，再重新检查。

## 分发前注意事项

- 分发包不能包含 `~/.cli-proxy-api/`、`~/.codex/`、`.env`、本地 `config.yaml`、私钥或证书文件。
- `src-tauri/resources/config.yaml` 只能作为空 key 模板保留，`api-keys` 和 `codex-api-key` 必须为空。
