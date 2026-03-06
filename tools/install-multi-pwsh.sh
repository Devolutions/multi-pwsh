#!/usr/bin/env bash
set -euo pipefail

repo_owner="Devolutions"
repo_name="multi-pwsh"

version="${1:-latest}"
install_home="${MULTI_PWSH_HOME:-${HOME}/.pwsh}"
bin_dir="${MULTI_PWSH_BIN_DIR:-${install_home}/bin}"

if [[ "${version}" == "latest" ]]; then
  release_path="latest/download"
  display_version="latest"
else
  if [[ "${version}" != v* ]]; then
    version="v${version}"
  fi
  release_path="download/${version}"
  display_version="${version}"
fi

uname_s="$(uname -s)"
case "${uname_s}" in
  Linux) os="linux" ;;
  Darwin) os="macos" ;;
  *)
    echo "Unsupported OS: ${uname_s}. Supported OS: Linux, macOS." >&2
    exit 1
    ;;
esac

uname_m="$(uname -m)"
case "${uname_m}" in
  x86_64 | amd64) arch="x64" ;;
  aarch64 | arm64) arch="arm64" ;;
  *)
    echo "Unsupported architecture: ${uname_m}. Supported arch: x86_64/amd64, aarch64/arm64." >&2
    exit 1
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but was not found in PATH." >&2
  exit 1
fi

if ! command -v unzip >/dev/null 2>&1; then
  echo "unzip is required but was not found in PATH." >&2
  exit 1
fi

asset="multi-pwsh-${os}-${arch}.zip"
download_url="https://github.com/${repo_owner}/${repo_name}/releases/${release_path}/${asset}"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

archive_path="${tmp_dir}/${asset}"
extract_dir="${tmp_dir}/extract"

echo "Downloading ${asset} (${display_version})..."
curl -fsSL "${download_url}" -o "${archive_path}"

mkdir -p "${extract_dir}"
unzip -q "${archive_path}" -d "${extract_dir}"

binary_source="${extract_dir}/multi-pwsh"
if [[ ! -f "${binary_source}" ]]; then
  echo "Archive did not contain expected binary: multi-pwsh" >&2
  exit 1
fi

mkdir -p "${bin_dir}"
if command -v install >/dev/null 2>&1; then
  install -m 0755 "${binary_source}" "${bin_dir}/multi-pwsh"
else
  cp "${binary_source}" "${bin_dir}/multi-pwsh"
  chmod 0755 "${bin_dir}/multi-pwsh"
fi

if [[ ":${PATH}:" != *":${bin_dir}:"* ]]; then
  export PATH="${bin_dir}:${PATH}"
fi

profile_candidates=()
if [[ "${SHELL:-}" == *"zsh"* ]]; then
  profile_candidates+=("${HOME}/.zshrc")
fi
if [[ "${SHELL:-}" == *"bash"* ]]; then
  profile_candidates+=("${HOME}/.bashrc")
fi
profile_candidates+=("${HOME}/.profile")

profile_file=""
for candidate in "${profile_candidates[@]}"; do
  if [[ -f "${candidate}" ]]; then
    profile_file="${candidate}"
    break
  fi
done

if [[ -z "${profile_file}" ]]; then
  profile_file="${profile_candidates[0]}"
fi

escaped_bin_dir="${bin_dir//\\/\\\\}"
escaped_bin_dir="${escaped_bin_dir//\"/\\\"}"
escaped_bin_dir="${escaped_bin_dir//\$/\\$}"
profile_line="export PATH=\"${escaped_bin_dir}:\$PATH\""

touch "${profile_file}"
if grep -Fq "${profile_line}" "${profile_file}"; then
  path_status="PATH already contains ${bin_dir} in ${profile_file}"
else
  {
    echo ""
    echo "# Added by multi-pwsh installer"
    echo "${profile_line}"
  } >>"${profile_file}"
  path_status="Added ${bin_dir} to PATH in ${profile_file}"
fi

echo "Installed multi-pwsh to ${bin_dir}/multi-pwsh"
echo "${path_status}"
echo "Run: multi-pwsh --help"