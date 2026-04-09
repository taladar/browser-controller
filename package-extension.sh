#!/bin/bash

set -e -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXTENSION_DIR="${SCRIPT_DIR}/extension"
DIST_DIR="${SCRIPT_DIR}/dist"

version="$(jq -r '.version' "${EXTENSION_DIR}/manifest.json")"

mkdir -p "${DIST_DIR}"

# Collect all files except browser-specific manifest variants; manifest.json
# (the Firefox manifest) is included via the tmpdir copy below.
mapfile -t common_files < <(find "${EXTENSION_DIR}" -maxdepth 1 -type f -printf '%f\n' | grep -v '^manifest\.' | sort)

echo "Packaging extension version ${version} with files: ${common_files[*]}"

tmpdir=$(mktemp -d)
trap 'rm -rf "${tmpdir}"' EXIT

# Copy common files into the staging directory.
for f in "${common_files[@]}"; do
  cp "${EXTENSION_DIR}/${f}" "${tmpdir}/${f}"
done

# Firefox: manifest.json already contains the Firefox-specific settings
# (titlePreface / sessions support, browser_specific_settings/gecko).
# .xpi is a zip file with a different extension.
cp "${EXTENSION_DIR}/manifest.json" "${tmpdir}/manifest.json"
xpi_path="${DIST_DIR}/browser-controller-${version}.xpi"
(cd "${tmpdir}" && zip -q -r "${xpi_path}" .)
echo "Firefox: ${xpi_path}"

# Chrome/Chromium/Edge/Brave: use manifest.chrome.json which omits the
# sessions permission (unused on Chrome, as titlePreface is Firefox-only).
cp "${EXTENSION_DIR}/manifest.chrome.json" "${tmpdir}/manifest.json"
zip_path="${DIST_DIR}/browser-controller-${version}.zip"
(cd "${tmpdir}" && zip -q -r "${zip_path}" .)
echo "Chrome:  ${zip_path}"
