#!/bin/bash

set -e -u

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <major|minor|patch>" >&2
  exit 1
fi

level="$1"

cargo set-version --bump "${level}"

version="$(cargo get package.version)"
debian_package_name="browser-controller"
debian_package_revision="$(cargo metadata --format-version 1 --no-deps | jq -r -C '.packages[] | select(.name == "browser-controller") | .metadata.deb.revision')"

git cliff --prepend CHANGELOG.md -u -t "browser_controller_${version}"
git cliff --config cliff-debian.toml --prepend changelog -u -t "browser_controller_${version}" --context --output context.json
jq < \
context.json \
  --arg debian_package_name "${debian_package_name}" \
  --arg debian_package_revision "${debian_package_revision}" \
  '.[0] += { "extra": { "debian_package_name": $debian_package_name, "debian_package_revision": $debian_package_revision }}' \
  >full_context.json
git cliff --config cliff-debian.toml --prepend changelog -u -t "browser_controller_${version}" --from-context full_context.json
tail -n +2 changelog | sponge changelog
rm context.json full_context.json

rumdl fmt --fix CHANGELOG.md

cargo build

git add changelog CHANGELOG.md Cargo.toml Cargo.lock

git commit -m "chore(release): Release version ${version}"

git tag "browser_controller_${version}"

for remote in $(git remote); do
  git push "${remote}"
  git push "${remote}" "browser_controller_${version}"
done
cargo publish --dry-run
