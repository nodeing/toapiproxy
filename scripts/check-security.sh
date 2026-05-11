#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"

failures=0
warnings=0

fail() {
  printf 'ERROR: %s\n' "$*" >&2
  failures=$((failures + 1))
}

warn() {
  printf 'WARN: %s\n' "$*" >&2
  warnings=$((warnings + 1))
}

is_env_example() {
  case "$1" in
    *.env.example|*.env.sample|*.env.template|*.example.env|*.sample.env)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

check_forbidden_path() {
  local path="$1"
  local normalized="${path#./}"

  if [[ "$normalized" =~ (^|/)\.env($|\.) ]] && ! is_env_example "$normalized"; then
    fail "$normalized is a local environment file. Do not commit it."
  fi

  if [[ "$normalized" =~ \.(pem|key|p12|pfx|jks|keystore|mobileprovision)$ ]]; then
    fail "$normalized looks like a private key, certificate, or signing credential."
  fi

  if [[ "$normalized" =~ (^|/)auths/.*\.(json|ya?ml|toml)$ ]]; then
    fail "$normalized is an auth data file."
  fi

  if [[ "$normalized" =~ (^|/)(auth|token|tokens|credential|credentials|secret|secrets)\.(json|ya?ml|toml)$ ]]; then
    fail "$normalized looks like a saved credential file."
  fi

  if [[ "$normalized" =~ (^|/)(config\.local|.*\.local)\.(json|ya?ml|toml)$ ]]; then
    fail "$normalized looks like a local private config file."
  fi

  case "$normalized" in
    config.yaml|config.yml|codex-config-profiles.json|claude-config-profiles.json|backend-usage.json|usage-statistics/backend-usage.json)
      fail "$normalized looks like local runtime data or a private config file."
      ;;
  esac
}

known_secret_regex='-----BEGIN (RSA |DSA |EC |OPENSSH |PGP )?PRIVATE KEY-----|(^|[^A-Z0-9])(AKIA|ASIA)[0-9A-Z]{16}([^A-Z0-9]|$)|(^|[^A-Za-z0-9_-])sk-(proj-)?[A-Za-z0-9_-]{20,}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9_-])sk-ant-[A-Za-z0-9_-]{20,}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9_])(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{30,}([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])github_pat_[A-Za-z0-9_]{20,}([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_-])AIza[0-9A-Za-z_-]{35}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9-])xox[baprs]-[A-Za-z0-9-]{20,}([^A-Za-z0-9-]|$)|(^|[^A-Za-z0-9_-])eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}([^A-Za-z0-9_-]|$)'

scan_text_stream() {
  local label="$1"
  local source="$2"
  local hits
  hits="$(LC_ALL=C grep -nE -- "$known_secret_regex" "$source" 2>/dev/null || true)"
  if [[ -n "$hits" ]]; then
    while IFS= read -r hit; do
      [[ -z "$hit" ]] && continue
      fail "$label:${hit%%:*} contains a known secret pattern."
    done <<< "$hits"
  fi
}

scan_file_content() {
  local path="$1"
  local label="$2"

  if [[ ! -f "$path" ]]; then
    return
  fi

  if LC_ALL=C grep -Iq . "$path" 2>/dev/null; then
    scan_text_stream "$label" "$path"
    return
  fi

  if command -v strings >/dev/null 2>&1; then
    local hits
    hits="$(strings -a "$path" 2>/dev/null | LC_ALL=C grep -nE -- "$known_secret_regex" || true)"
    if [[ -n "$hits" ]]; then
      while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        fail "$label:binary-string:${hit%%:*} contains a known secret pattern."
      done <<< "$hits"
    fi
  fi
}

check_bundled_config() {
  local config_path="$1"
  local label="$2"

  if [[ ! -f "$config_path" ]]; then
    fail "$label is missing."
    return
  fi

  if ! awk '
    /^[[:space:]]*#/ { next }
    {
      line = $0
      sub(/[[:space:]]+#.*$/, "", line)

      if (in_key_list) {
        if (line ~ /^[[:space:]]*-[[:space:]]*\S/) {
          exit 1
        }
        if (line !~ /^[[:space:]]+/) {
          in_key_list = 0
        }
      }

      if (line ~ /^[[:space:]]*-[[:space:]]*api-key:[[:space:]]*/) {
        value = line
        sub(/^[[:space:]]*-[[:space:]]*api-key:[[:space:]]*/, "", value)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
        if (value != "" && value != "\"\"" && value != "''") {
          exit 1
        }
      }

      if (line ~ /^[[:space:]]*(api-keys|codex-api-key):[[:space:]]*$/) {
        in_key_list = 1
      } else if (line ~ /^[[:space:]]*(api-keys|codex-api-key):[[:space:]]*/) {
        value = line
        sub(/^[[:space:]]*(api-keys|codex-api-key):[[:space:]]*/, "", value)
        gsub(/[[:space:]]/, "", value)
        if (value != "[]") {
          exit 1
        }
      }
    }
  ' "$config_path"; then
    fail "$label contains non-empty api-keys, codex-api-key, or upstream api-key values."
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

add_path() {
  local path="$1"
  [[ -z "$path" ]] && return
  printf '%s\n' "$path" >> "$tmp_dir/paths"
}

while IFS= read -r -d '' path; do
  add_path "$path"
done < <(git ls-files -z)

while IFS= read -r -d '' path; do
  add_path "$path"
done < <(git ls-files --others --exclude-standard -z)

while IFS= read -r -d '' path; do
  add_path "$path"
done < <(git diff --cached --name-only -z --diff-filter=ACMR)

while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  check_forbidden_path "$path"
  scan_file_content "$path" "working:$path"

  if [[ "$path" =~ ^docs/img/.*\.(png|jpg|jpeg|webp)$ ]]; then
    warn "$path is a documentation image. Manually confirm it does not show personal account data."
  fi
done < <(sort -u "$tmp_dir/paths")

check_bundled_config "src-tauri/resources/config.yaml" "src-tauri/resources/config.yaml"

while IFS= read -r -d '' staged_path; do
  check_forbidden_path "$staged_path"

  staged_file="$tmp_dir/staged"
  if git show ":$staged_path" > "$staged_file" 2>/dev/null; then
    scan_file_content "$staged_file" "staged:$staged_path"
    if [[ "$staged_path" == "src-tauri/resources/config.yaml" ]]; then
      check_bundled_config "$staged_file" "staged:src-tauri/resources/config.yaml"
    fi
  fi
done < <(git diff --cached --name-only -z --diff-filter=ACMR)

if (( failures > 0 )); then
  printf '\nSecurity check failed with %d problem(s).\n' "$failures" >&2
  exit 1
fi

printf 'Security check passed'
if (( warnings > 0 )); then
  printf ' with %d warning(s)' "$warnings"
fi
printf '.\n'
