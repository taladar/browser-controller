#!/bin/bash

set -e -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXTENSION_DIR="${SCRIPT_DIR}/extension"
DIST_DIR="${SCRIPT_DIR}/dist"

version="$(jq -r '.version' "${EXTENSION_DIR}/manifest.json")"

mkdir -p "${DIST_DIR}"

# Collect all files to package (everything in the extension directory).
mapfile -t files < <(find "${EXTENSION_DIR}" -maxdepth 1 -type f -printf '%f\n' | sort)

echo "Packaging extension version ${version} with files: ${files[*]}"

# Firefox: .xpi is a zip with a different extension.
xpi_path="${DIST_DIR}/browser-controller-${version}.xpi"
(cd "${EXTENSION_DIR}" && zip -q -r "${xpi_path}" "${files[@]}")
echo "Firefox: ${xpi_path}"

# Chrome/Chromium/Edge/Brave: plain .zip.
zip_path="${DIST_DIR}/browser-controller-${version}.zip"
(cd "${EXTENSION_DIR}" && zip -q -r "${zip_path}" "${files[@]}")
echo "Chrome:  ${zip_path}"
