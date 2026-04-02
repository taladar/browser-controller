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

for p in "${workspace_crates[@]}"; do
  cargo set-version --bump "${level}" -p "${p}"
done

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
  jq < \
  context.json \
    --arg debian_package_name "${debian_package_name}" \
    --arg debian_package_revision "${debian_package_revision}" \
    '.[0] += { "extra": { "debian_package_name": $debian_package_name, "debian_package_revision": $debian_package_revision }}' \
    >full_context.json
  git cliff --config cliff-debian.toml --prepend changelog -u -t "${p_tag_basename}_${version}" --from-context full_context.json
  tail -n +2 changelog | sponge changelog
  rm context.json full_context.json
  popd >/dev/null
done

cargo build

git add Cargo.toml Cargo.lock

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

for remote in $(git remote); do
  git push "${remote}"
  for p in "${workspace_crates[@]}"; do
    p_tag_basename="${p//-/_}"
    pushd "${p}" >/dev/null
    version="$(cargo get package.version)"
    git push "${remote}" "${p_tag_basename}_${version}"
    popd >/dev/null
  done
done
