#!/bin/sh
# Herdr build step: install a prebuilt release, or build from source when that fails.
# Set HERDR_LENS_FROM_SOURCE=1 to skip the download.
set -eu

REPO="frankittee/herdr-lens"
cd "$(dirname "$0")/.."

log() { echo "herdr-lens: $*" >&2; }

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$arch" in
    arm64 | aarch64) arch=aarch64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *) return 1 ;;
  esac
  case "$os" in
    Darwin) echo "$arch-apple-darwin" ;;
    Linux) echo "$arch-unknown-linux-musl" ;;
    *) return 1 ;;
  esac
}

manifest_version() {
  sed -n 's/^version *= *"\(.*\)"/\1/p' herdr-plugin.toml | head -n 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

# Uses gh when available so private releases work; falls back to anonymous curl.
fetch_assets() {
  tag=$1 asset=$2 dir=$3
  if command -v gh >/dev/null 2>&1 &&
    gh release download "$tag" --repo "$REPO" --dir "$dir" \
      --pattern "$asset" --pattern "$asset.sha256" >/dev/null 2>&1; then
    return 0
  fi
  command -v curl >/dev/null 2>&1 || return 1
  base="https://github.com/$REPO/releases/download/$tag"
  curl -fsSL -o "$dir/$asset" "$base/$asset" &&
    curl -fsSL -o "$dir/$asset.sha256" "$base/$asset.sha256"
}

install_prebuilt() {
  target=$(detect_target) || { log "no prebuilt binary for $(uname -s) $(uname -m)"; return 1; }
  version=$(manifest_version)
  [ -n "$version" ] || { log "cannot read version from herdr-plugin.toml"; return 1; }
  tag="v$version"
  asset="herdr-lens-$target.tar.gz"

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT

  log "downloading $asset from release $tag"
  fetch_assets "$tag" "$asset" "$tmp" || { log "download failed"; return 1; }

  expected=$(cut -d ' ' -f 1 "$tmp/$asset.sha256")
  actual=$(sha256_of "$tmp/$asset")
  [ "$expected" = "$actual" ] || { log "checksum mismatch for $asset"; return 1; }

  # set -e is inactive inside an `if` condition, so every step checks its own status.
  mkdir "$tmp/pkg" && tar -xzf "$tmp/$asset" -C "$tmp/pkg" || { log "cannot extract $asset"; return 1; }
  [ -x "$tmp/pkg/herdr-lens" ] && [ -f "$tmp/pkg/dist/index.html" ] ||
    { log "unexpected archive layout"; return 1; }

  mkdir -p target/release web &&
    rm -rf web/dist &&
    mv "$tmp/pkg/herdr-lens" target/release/herdr-lens &&
    mv "$tmp/pkg/dist" web/dist || { log "cannot install files"; return 1; }
  log "installed prebuilt $target binary"
}

build_from_source() {
  command -v mise >/dev/null 2>&1 || { log "local build needs mise (https://mise.jdx.dev)"; exit 1; }
  log "building from source"
  mise x -- mbx build --release
  mise x -- bun install --cwd web --frozen-lockfile
  mise x -- bun run --cwd web build
}

if [ "${HERDR_LENS_FROM_SOURCE:-}" != 1 ] && (install_prebuilt); then
  exit 0
fi
build_from_source
