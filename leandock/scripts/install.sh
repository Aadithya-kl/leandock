#!/usr/bin/env bash
set -euo pipefail

REPO="${LEANDOCK_REPO:-leandock/leandock}"
INSTALL_DIR="${LEANDOCK_INSTALL_DIR:-/usr/local/bin}"
VERSION="${LEANDOCK_VERSION:-latest}"
INSTALL_DOCKER_PLUGIN=false

for arg in "$@"; do
  case "${arg}" in
    --docker-plugin) INSTALL_DOCKER_PLUGIN=true ;;
    --help|-h)
      echo "Usage: install.sh [--docker-plugin]"
      echo "  --docker-plugin   Also symlink ~/.docker/cli-plugins/docker-leandock so `docker leandock` works."
      exit 0
      ;;
  esac
done

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "${arch}" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)
      echo "Error: Unsupported CPU architecture: ${arch}" >&2
      exit 1
      ;;
  esac

  case "${os}" in
    linux)  echo "${arch}-unknown-linux-musl" ;;
    darwin) echo "${arch}-apple-darwin" ;;
    *)
      echo "Error: Unsupported OS: ${os}. For Windows, use scripts/install.ps1." >&2
      exit 1
      ;;
  esac
}

verify_sha256() {
  local file="$1"
  local checksum_file="$2"
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "${file}")" && sha256sum -c "$(basename "${checksum_file}")")
  elif command -v shasum >/dev/null 2>&1; then
    local expected actual
    expected="$(awk '{print $1}' "${checksum_file}")"
    actual="$(shasum -a 256 "${file}" | awk '{print $1}')"
    if [ "${expected}" != "${actual}" ]; then
      echo "Error: SHA256 checksum mismatch!" >&2
      exit 1
    fi
  else
    echo "Warning: neither sha256sum nor shasum found; skipping checksum verification." >&2
  fi
}

main() {
  local target tag base_url archive tmp_dir
  target="$(detect_platform)"

  if [ "${VERSION}" = "latest" ]; then
    tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')"
  else
    tag="${VERSION}"
  fi

  if [ -z "${tag}" ]; then
    echo "Error: Could not resolve latest LeanDock release tag." >&2
    exit 1
  fi

  archive="leandock-${tag}-${target}.tar.gz"
  base_url="https://github.com/${REPO}/releases/download/${tag}"
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT

  echo "Downloading LeanDock ${tag} (${target})..."
  curl -fsSL "${base_url}/${archive}" -o "${tmp_dir}/${archive}"
  curl -fsSL "${base_url}/${archive}.sha256" -o "${tmp_dir}/${archive}.sha256"

  echo "Verifying SHA256 checksum..."
  verify_sha256 "${tmp_dir}/${archive}" "${tmp_dir}/${archive}.sha256"

  tar -xzf "${tmp_dir}/${archive}" -C "${tmp_dir}"

  if [ -w "${INSTALL_DIR}" ]; then
    install -m 0755 "${tmp_dir}/leandock" "${INSTALL_DIR}/leandock"
  else
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo install -m 0755 "${tmp_dir}/leandock" "${INSTALL_DIR}/leandock"
  fi

  if [ "${INSTALL_DOCKER_PLUGIN}" = "true" ]; then
    local plugin_dir="${HOME}/.docker/cli-plugins"
    mkdir -p "${plugin_dir}"
    ln -sf "${INSTALL_DIR}/leandock" "${plugin_dir}/docker-leandock"
    echo "Installed Docker CLI plugin symlink: ${plugin_dir}/docker-leandock"
  fi

  echo "Installed LeanDock successfully: $(command -v leandock || echo "${INSTALL_DIR}/leandock")"
  "${INSTALL_DIR}/leandock" --version
}

main "$@"
