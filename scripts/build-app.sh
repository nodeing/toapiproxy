#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-app.sh [options]

Build the Tauri desktop app and copy artifacts into build/.

Options:
  --target <triple>           Rust target triple to build for
  --bundles <list>            Comma-separated Tauri bundle types
  --signing-identity <value>  Export APPLE_SIGNING_IDENTITY for the build
  --no-sign                   Skip code signing entirely
  --no-copy-artifacts         Leave the generated bundle only under src-tauri/target
  -h, --help                  Show this help text

Examples:
  scripts/build-app.sh --target aarch64-apple-darwin --bundles app,dmg --no-sign
  scripts/build-app.sh --target x86_64-apple-darwin --bundles app,dmg --signing-identity -
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
build_dir="$repo_root/build"
tauri_config_path="$repo_root/src-tauri/tauri.conf.json"

target_triple=""
bundles=""
copy_artifacts=1
no_sign=0
signing_identity="${MACOS_SIGNING_IDENTITY:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target_triple="${2:-}"
      shift 2
      ;;
    --bundles)
      bundles="${2:-}"
      shift 2
      ;;
    --signing-identity)
      signing_identity="${2:-}"
      shift 2
      ;;
    --no-sign)
      no_sign=1
      shift
      ;;
    --no-copy-artifacts)
      copy_artifacts=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 1
  fi
}

assert_bundled_config_has_no_real_secrets() {
  local config_path="$1"

  if [[ ! -f "$config_path" ]]; then
    echo "Bundled config template not found at $config_path" >&2
    exit 1
  fi

  if awk '
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*-[[:space:]]*api-key:[[:space:]]*/ {
      value = $0
      sub(/^[[:space:]]*-[[:space:]]*api-key:[[:space:]]*/, "", value)
      sub(/[[:space:]]+#.*$/, "", value)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      if (value != "" && value != "\"\"" && value != "'\'''\''") {
        exit 1
      }
    }
  ' "$config_path"; then
    :
  else
    cat >&2 <<EOF
Bundled config template contains non-empty upstream API keys:
  $config_path

Move real credentials to the user's local runtime config after first launch.
The packaged config.yaml must remain a secret-free template.
EOF
    exit 1
  fi

  if awk '
    /^[[:space:]]*#/ { next }
    {
      if (in_api_keys) {
        if ($0 ~ /^[[:space:]]*-[[:space:]]*\S/) {
          exit 1
        }
        if ($0 !~ /^[[:space:]]+/) {
          in_api_keys = 0
        }
      }

      if ($0 ~ /^[[:space:]]*api-keys:[[:space:]]*$/) {
        in_api_keys = 1
      } else if ($0 ~ /^[[:space:]]*api-keys:[[:space:]]*\[[^]]+\]/) {
        exit 1
      }
    }
  ' "$config_path"; then
    :
  else
    cat >&2 <<EOF
Bundled config template contains non-empty client API keys:
  $config_path

Move real credentials to the user's local runtime config after first launch.
The packaged config.yaml must remain a secret-free template.
EOF
    exit 1
  fi
}

detect_host_target() {
  if rustc --print host-tuple >/dev/null 2>&1; then
    rustc --print host-tuple
    return 0
  fi

  local rust_host
  rust_host="$(rustc -Vv | awk '/^host:/ { print $2 }')"
  if [[ -n "$rust_host" ]]; then
    echo "$rust_host"
    return 0
  fi

  local uname_s uname_m
  uname_s="$(uname -s)"
  uname_m="$(uname -m)"
  case "$uname_s:$uname_m" in
    Darwin:arm64|Darwin:aarch64)
      echo "aarch64-apple-darwin"
      ;;
    Darwin:x86_64)
      echo "x86_64-apple-darwin"
      ;;
    Linux:x86_64)
      echo "x86_64-unknown-linux-gnu"
      ;;
    Linux:arm64|Linux:aarch64)
      echo "aarch64-unknown-linux-gnu"
      ;;
    *)
      echo "Unsupported host platform: $uname_s/$uname_m" >&2
      exit 1
      ;;
  esac
}

ensure_rust_target_installed() {
  local required=("$@")
  local installed

  if ! command -v rustup >/dev/null 2>&1; then
    return 0
  fi

  installed="$(rustup target list --installed)"
  for triple in "${required[@]}"; do
    if [[ "$triple" == "universal-apple-darwin" ]]; then
      continue
    fi
    if ! grep -qx "$triple" <<<"$installed"; then
      echo "Missing Rust target '$triple'. Install it with:" >&2
      echo "  rustup target add $triple" >&2
      exit 1
    fi
  done
}

artifact_label_for_target() {
  case "$1" in
    aarch64-apple-darwin)
      echo "macos_apple_silicon"
      ;;
    x86_64-apple-darwin)
      echo "macos_intel"
      ;;
    universal-apple-darwin)
      echo "macos_universal"
      ;;
    x86_64-unknown-linux-gnu)
      echo "linux_x64"
      ;;
    aarch64-unknown-linux-gnu)
      echo "linux_arm64"
      ;;
    *)
      echo "generic_host"
      ;;
  esac
}

extract_tauri_json_value() {
  local key="$1"
  sed -nE 's/.*"'"$key"'":[[:space:]]*"([^"]+)".*/\1/p' "$tauri_config_path" | head -n 1
}

sanitize_artifact_value() {
  local value="$1"
  value="${value// /_}"
  value="$(printf '%s' "$value" | tr -cd '[:alnum:]_.-')"
  value="${value##_}"
  value="${value%%_}"
  if [[ -z "$value" ]]; then
    value="app"
  fi
  printf '%s' "$value"
}

copy_named_file() {
  local source_path="$1"
  local dest_path="$2"

  if [[ -f "$source_path" ]]; then
    cp "$source_path" "$dest_path"
    echo "Copied $(basename "$source_path") -> $dest_path"
  fi
}

require_command cargo
require_command rustc

if [[ ! -f "$tauri_config_path" ]]; then
  echo "tauri.conf.json not found at $tauri_config_path" >&2
  exit 1
fi

assert_bundled_config_has_no_real_secrets "$repo_root/src-tauri/resources/config.yaml"

target_triple="${target_triple:-$(detect_host_target)}"
product_name="$(extract_tauri_json_value "productName")"
version="$(extract_tauri_json_value "version")"
product_name="$(sanitize_artifact_value "${product_name:-app}")"
version="$(sanitize_artifact_value "${version:-0.0.0}")"

if [[ -z "$bundles" ]]; then
  case "$target_triple" in
    *apple-darwin|universal-apple-darwin)
      bundles="app,dmg"
      ;;
  esac
fi

if [[ "$target_triple" == *apple-darwin || "$target_triple" == "universal-apple-darwin" ]]; then
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "macOS bundles must be built on a Mac host." >&2
    exit 1
  fi
fi

if [[ "$target_triple" == "universal-apple-darwin" ]]; then
  ensure_rust_target_installed "aarch64-apple-darwin" "x86_64-apple-darwin"
else
  ensure_rust_target_installed "$target_triple"
fi

"$script_dir/build-cli-proxy.sh" --target "$target_triple"

cargo_args=(tauri build --ci)
if [[ -n "$target_triple" ]]; then
  cargo_args+=(--target "$target_triple")
fi
if [[ -n "$bundles" ]]; then
  cargo_args+=(--bundles "$bundles")
fi
if [[ $no_sign -eq 1 ]]; then
  cargo_args+=(--no-sign)
elif [[ -n "$signing_identity" ]]; then
  export APPLE_SIGNING_IDENTITY="$signing_identity"
fi

echo "Running: cargo ${cargo_args[*]}"
(
  cd "$repo_root"
  cargo "${cargo_args[@]}"
)

if [[ -n "$target_triple" ]]; then
  bundle_root="$repo_root/src-tauri/target/$target_triple/release/bundle"
else
  bundle_root="$repo_root/src-tauri/target/release/bundle"
fi

if [[ $copy_artifacts -eq 0 ]]; then
  echo "Artifacts left in $bundle_root"
  exit 0
fi

artifact_subdir="$(artifact_label_for_target "$target_triple")"
artifact_dest="$build_dir/dist"
mkdir -p "$artifact_dest"
artifact_base_name="${product_name}_${version}_${artifact_subdir}"
copied_count=0

case "$target_triple" in
  *apple-darwin|universal-apple-darwin)
    if [[ -d "$bundle_root/dmg" ]]; then
      while IFS= read -r dmg_path; do
        copy_named_file "$dmg_path" "$artifact_dest/${artifact_base_name}.dmg"
        copied_count=$((copied_count + 1))
      done < <(find "$bundle_root/dmg" -maxdepth 1 -type f -name '*.dmg' | sort)
    fi
    ;;
  *)
    while IFS= read -r file_path; do
      copy_named_file "$file_path" "$artifact_dest/${artifact_base_name}"
      copied_count=$((copied_count + 1))
    done < <(find "$bundle_root" -maxdepth 2 -type f | sort)
    ;;
esac

if [[ $copied_count -eq 0 ]]; then
  echo "No distributable artifacts were copied from $bundle_root" >&2
  exit 1
fi

echo "Bundle artifacts copied to $artifact_dest"
