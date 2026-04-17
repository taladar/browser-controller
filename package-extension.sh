#!/bin/bash

set -e -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXTENSION_DIR="${SCRIPT_DIR}/extension"
DIST_DIR="${SCRIPT_DIR}/dist"

version="$(jq -r '.version' "${EXTENSION_DIR}/manifest.json")"

mkdir -p "${DIST_DIR}"

# Files used only by Chrome (offscreen keepalive for service worker).
chrome_only_files=(offscreen.html offscreen.js)

# Collect all non-manifest files.
mapfile -t all_files < <(find "${EXTENSION_DIR}" -maxdepth 1 -type f -printf '%f\n' | grep -v '^manifest\.' | sort)

# Common files shared by both browsers (exclude Chrome-only files).
common_files=()
for f in "${all_files[@]}"; do
  skip=false
  for cf in "${chrome_only_files[@]}"; do
    if [[ "${f}" == "${cf}" ]]; then
      skip=true
      break
    fi
  done
  if ! "${skip}"; then common_files+=("${f}"); fi
done

echo "Packaging extension version ${version}"
echo "  Common files:     ${common_files[*]}"
echo "  Chrome-only files: ${chrome_only_files[*]}"

tmpdir=$(mktemp -d)
trap 'rm -rf "${tmpdir}"' EXIT

# --- Firefox (.xpi) ---
# manifest.json already contains Firefox-specific settings
# (titlePreface / sessions support, browser_specific_settings/gecko).
for f in "${common_files[@]}"; do
  cp "${EXTENSION_DIR}/${f}" "${tmpdir}/${f}"
done
cp "${EXTENSION_DIR}/manifest.json" "${tmpdir}/manifest.json"
xpi_path="${DIST_DIR}/browser-controller-${version}.xpi"
(cd "${tmpdir}" && zip -q -r "${xpi_path}" .)
echo "Firefox: ${xpi_path}"

# Clean staging dir for Chrome build.
rm -f "${tmpdir}"/*

# --- Chrome/Chromium/Edge/Brave (.zip) ---
# Use manifest.chrome.json which includes Chrome-specific permissions
# (tabGroups, webRequestAuthProvider, offscreen, alarms).
for f in "${common_files[@]}" "${chrome_only_files[@]}"; do
  cp "${EXTENSION_DIR}/${f}" "${tmpdir}/${f}"
done
cp "${EXTENSION_DIR}/manifest.chrome.json" "${tmpdir}/manifest.json"
zip_path="${DIST_DIR}/browser-controller-${version}.zip"
(cd "${tmpdir}" && zip -q -r "${zip_path}" .)
echo "Chrome:  ${zip_path}"
