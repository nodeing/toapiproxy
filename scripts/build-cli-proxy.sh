#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-cli-proxy.sh [--target <triple>] [--output <path>]

Builds the bundled CLIProxyAPI backend binary for the requested target and
copies it to src-tauri/resources/cli-proxy-api by default.

Supported targets:
  aarch64-apple-darwin
  x86_64-apple-darwin
  universal-apple-darwin
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu

Examples:
  scripts/build-cli-proxy.sh --target aarch64-apple-darwin
  scripts/build-cli-proxy.sh --target x86_64-apple-darwin
  scripts/build-cli-proxy.sh --target universal-apple-darwin
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cli_project_dir="$repo_root/third_party/CLIProxyAPI"
resources_dir="$repo_root/src-tauri/resources"
go_cache_root="$repo_root/.cache/go"
go_build_cache="$go_cache_root/build"
go_mod_cache="$go_cache_root/mod"
artifact_dir="$repo_root/build/backend"

target_triple=""
output_path=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target_triple="${2:-}"
      shift 2
      ;;
    --output)
      output_path="${2:-}"
      shift 2
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

resolve_cli_proxy_commit() {
  local subtree_path="third_party/CLIProxyAPI"
  local subtree_message
  local split_commit
  local path_commit

  if ! command -v git >/dev/null 2>&1; then
    printf 'workspace'
    return 0
  fi

  subtree_message="$(git -C "$repo_root" log --format=%B -n 1 --grep="git-subtree-dir: $subtree_path" --fixed-strings 2>/dev/null || true)"
  split_commit="$(printf '%s\n' "$subtree_message" | awk '/^git-subtree-split:/ { print $2; exit }')"
  if [[ -n "$split_commit" ]]; then
    printf '%.8s' "$split_commit"
    return 0
  fi

  path_commit="$(git -C "$repo_root" log -1 --format=%h -- "$subtree_path" 2>/dev/null || true)"
  if [[ -n "$path_commit" ]]; then
    printf '%s' "$path_commit"
    return 0
  fi

  printf 'workspace'
}

detect_host_target() {
  if command -v rustc >/dev/null 2>&1; then
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

resolve_go_target() {
  local triple="$1"
  case "$triple" in
    aarch64-apple-darwin)
      echo "darwin arm64"
      ;;
    x86_64-apple-darwin)
      echo "darwin amd64"
      ;;
    x86_64-unknown-linux-gnu)
      echo "linux amd64"
      ;;
    aarch64-unknown-linux-gnu)
      echo "linux arm64"
      ;;
    *)
      echo "Unsupported target triple: $triple" >&2
      exit 1
      ;;
  esac
}

build_single_target() {
  local triple="$1"
  local out_path="$2"
  local go_target
  local goos
  local goarch

  go_target="$(resolve_go_target "$triple")"
  read -r goos goarch <<<"$go_target"

  mkdir -p "$(dirname "$out_path")"

  echo "Building CLIProxyAPI for $triple (GOOS=$goos GOARCH=$goarch)"
  (
    cd "$cli_project_dir"
    CGO_ENABLED=0 \
    GOTOOLCHAIN=local \
    GOCACHE="$go_build_cache" \
    GOMODCACHE="$go_mod_cache" \
    GOOS="$goos" \
    GOARCH="$goarch" \
    go build \
      -buildvcs=false \
      -trimpath \
      -ldflags "-s -w -X main.Version=workspace -X main.Commit=$commit -X main.BuildDate=$build_date" \
      -o "$out_path" \
      ./cmd/server
  )
  chmod +x "$out_path"
}

build_universal_macos_binary() {
  local out_path="$1"
  local tmp_dir="$2"
  local arm_out="$tmp_dir/cli-proxy-api-aarch64-apple-darwin"
  local intel_out="$tmp_dir/cli-proxy-api-x86_64-apple-darwin"

  require_command lipo

  build_single_target "aarch64-apple-darwin" "$arm_out"
  build_single_target "x86_64-apple-darwin" "$intel_out"

  echo "Merging macOS backend into a universal binary"
  lipo -create -output "$out_path" "$arm_out" "$intel_out"
  chmod +x "$out_path"
}

require_command go

if [[ ! -d "$cli_project_dir" ]]; then
  echo "CLIProxyAPI project not found at $cli_project_dir" >&2
  exit 1
fi

target_triple="${target_triple:-$(detect_host_target)}"
output_path="${output_path:-$resources_dir/cli-proxy-api}"

mkdir -p "$resources_dir" "$go_build_cache" "$go_mod_cache" "$artifact_dir"

build_date="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit="$(resolve_cli_proxy_commit)"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/toapiproxy-cli-build.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

artifact_path="$artifact_dir/cli-proxy-api-$target_triple"

if [[ "$target_triple" == "universal-apple-darwin" ]]; then
  build_universal_macos_binary "$artifact_path" "$tmp_dir"
else
  build_single_target "$target_triple" "$artifact_path"
fi

cp "$artifact_path" "$output_path"
chmod +x "$output_path"

echo "CLIProxyAPI binary updated at $output_path"
echo "Target artifact cached at $artifact_path"
