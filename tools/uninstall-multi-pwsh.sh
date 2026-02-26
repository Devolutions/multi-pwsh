#!/usr/bin/env bash
set -euo pipefail

install_root="${HOME}/.pwsh"
bin_dir="${install_root}/bin"
binary_path="${bin_dir}/multi-pwsh"

remove_profile_entries() {
  local profile_file="$1"

  [[ -f "${profile_file}" ]] || return 0

  local temp_file
  temp_file="$(mktemp)"

  awk '
    $0 == "# Added by multi-pwsh installer" { changed = 1; next }
    $0 == "export PATH=\"$HOME/.pwsh/bin:$PATH\"" { changed = 1; next }
    { print }
  ' "${profile_file}" > "${temp_file}"

  if ! cmp -s "${profile_file}" "${temp_file}"; then
    mv "${temp_file}" "${profile_file}"
    echo "Removed PATH profile entry from ${profile_file}"
  else
    rm -f "${temp_file}"
  fi
}

if [[ -f "${binary_path}" ]]; then
  rm -f "${binary_path}"
  echo "Removed ${binary_path}"
else
  echo "No installed binary found at ${binary_path}"
fi

if [[ ":${PATH}:" == *":${bin_dir}:"* ]]; then
  export PATH=":${PATH}:"
  export PATH="${PATH//:${bin_dir}:/}"
  export PATH="${PATH#:}"
  export PATH="${PATH%:}"
fi

remove_profile_entries "${HOME}/.zshrc"
remove_profile_entries "${HOME}/.bashrc"
remove_profile_entries "${HOME}/.profile"

if [[ -d "${bin_dir}" ]] && [[ -z "$(ls -A "${bin_dir}")" ]]; then
  rmdir "${bin_dir}"
  echo "Removed empty directory ${bin_dir}"
fi

echo "multi-pwsh uninstall complete"