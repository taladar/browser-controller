#!/bin/bash

set -e -u

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <major|minor|patch>" >&2
  exit 1
fi

level="$1"

declare -a generated_workspace_crates
# add any generated crates where the version can not be changed (e.g. from OpenAPI JSON or YAML) to this list
# so they will be skipped for the whole "bump version, generate changelogs"
generated_workspace_crates=()

declare -a workspace_crates
workspace_crates=()

for c in $(cargo get --delimiter LF --terminator LF workspace.members); do
  if [[ ${#generated_workspace_crates[*]} -gt 0 ]]; then
    for gc in "${generated_workspace_crates[@]}"; do
      if [[ "${c}" == "${gc}" ]]; then
        continue
      fi
    done
  fi
  workspace_crates+=("${c}")
done

declare -a workspace_binary_crates
workspace_binary_crates=()

for p in "${workspace_crates[@]}"; do
  if [[ -e "${p}/src/main.rs" ]] || [[ -d "${p}/src/bin" ]]; then
    workspace_binary_crates+=("${p}")
  fi
done

cargo set-version --bump "${level}"

# Read the new workspace version once; used for the extension manifest, workspace
# CHANGELOG, and the workspace-level release tag.
workspace_version="$(cargo get workspace.package.version)"

# Update extension/manifest.json to match the new workspace version.
jq --arg v "${workspace_version}" '.version = $v' extension/manifest.json | sponge extension/manifest.json

for p in "${workspace_crates[@]}"; do
  p_tag_basename="${p//-/_}"
  pushd "${p}" >/dev/null
  version="$(cargo get package.version)"
  git cliff --prepend CHANGELOG.md -u -t "${p_tag_basename}_${version}"
  rumdl fmt --fix CHANGELOG.md
  popd >/dev/null
done

for p in "${workspace_binary_crates[@]}"; do
  p_tag_basename="${p//-/_}"
  pushd "${p}" >/dev/null
  version="$(cargo get package.version)"
  package_name="$(cargo get package.name)"
  debian_package_name="$(cargo metadata --format-version 1 --no-deps | jq -r -C ".packages[] | select(.name == \"${package_name}\") | .metadata.deb.name")"
  debian_package_revision="$(cargo metadata --format-version 1 --no-deps | jq -r -C ".packages[] | select(.name == \"${package_name}\") | .metadata.deb.revision")"

  git cliff --config cliff-debian.toml --prepend changelog -u -t "${p_tag_basename}_${version}" --context --output context.json
  if [[ "$(cat context.json)" == "[]" ]]; then
    # No relevant commits for this package: prepend an empty section manually.
    {
      printf '%s (%s-%s) unstable; urgency=medium\n\n' \
        "${debian_package_name}" "${version}" "${debian_package_revision}"
      printf '  * No changes in this package; see other browser-controller components.\n\n'
      printf ' -- Matthias Hörmann <mhoermann@gmail.com>  %s\n\n' "$(date -R)"
      cat changelog
    } | sponge changelog
  else
    jq < \
    context.json \
      --arg debian_package_name "${debian_package_name}" \
      --arg debian_package_revision "${debian_package_revision}" \
      '.[0] += { "extra": { "debian_package_name": $debian_package_name, "debian_package_revision": $debian_package_revision }}' \
      >full_context.json
    git cliff --config cliff-debian.toml --prepend changelog -u -t "${p_tag_basename}_${version}" --from-context full_context.json
    tail -n +2 changelog | sponge changelog
    rm full_context.json
  fi
  rm context.json
  popd >/dev/null
done

git cliff --prepend CHANGELOG.md -u -t "browser_controller_${workspace_version}"
rumdl fmt --fix CHANGELOG.md

cargo build

"$(dirname "${BASH_SOURCE[0]}")/package-extension.sh"

git add Cargo.toml Cargo.lock CHANGELOG.md extension/manifest.json

for p in "${workspace_crates[@]}"; do
  pushd "${p}" >/dev/null
  git add CHANGELOG.md Cargo.toml
  popd >/dev/null
done

for p in "${workspace_binary_crates[@]}"; do
  pushd "${p}" >/dev/null
  git add changelog
  popd >/dev/null
done

git commit -m "chore(release): Release new version"

for p in "${workspace_crates[@]}"; do
  p_tag_basename="${p//-/_}"
  pushd "${p}" >/dev/null
  version="$(cargo get package.version)"
  git tag "${p_tag_basename}_${version}"
  popd >/dev/null
done

# Workspace-level release tag, triggers the GitHub release workflow.
git tag "browser_controller_${workspace_version}"

for remote in $(git remote); do
  git push "${remote}"
  git push "${remote}" "browser_controller_${workspace_version}"
  for p in "${workspace_crates[@]}"; do
    p_tag_basename="${p//-/_}"
    pushd "${p}" >/dev/null
    version="$(cargo get package.version)"
    git push "${remote}" "${p_tag_basename}_${version}"
    popd >/dev/null
  done
done
