#!/usr/bin/env bash
# check-version.sh — Verifica paridad de versión entre Cargo.toml, npm/package.json,
# último tag git y CHANGELOG.md.
# Uso: ./scripts/check-version.sh

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo_version=$(sed -nE 's/^version *= *"([^"]+)".*/\1/p' "${REPO_DIR}/Cargo.toml" | head -1)
npm_version=$(grep '"version"' "${REPO_DIR}/npm/package.json" | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' | head -1)
latest_tag=$(git -C "${REPO_DIR}" tag --sort=-v:refname | head -1 | sed 's/^v//')
# Extraer versión del CHANGELOG (primera ocurrencia tras "## [")
changelog_version=$(grep -E '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' "${REPO_DIR}/CHANGELOG.md" | head -1 | sed -E 's/^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*/\1/')

echo "Cargo.toml:    ${cargo_version}"
echo "npm/package.json: ${npm_version}"
echo "Git tag:       ${latest_tag}"
echo "CHANGELOG.md:  ${changelog_version}"

failed=0
if [ "${cargo_version}" != "${npm_version}" ]; then
  echo "FALLO: Cargo (${cargo_version}) ≠ npm (${npm_version})" >&2
  failed=1
fi
if [ "${cargo_version}" != "${latest_tag}" ]; then
  echo "FALLO: Cargo (${cargo_version}) ≠ tag (${latest_tag})" >&2
  failed=1
fi
if [ "${cargo_version}" != "${changelog_version}" ]; then
  echo "FALLO: Cargo (${cargo_version}) ≠ CHANGELOG (${changelog_version})" >&2
  failed=1
fi

if [ "${failed}" -eq 0 ]; then
  echo "OK: versiones alineadas (${cargo_version})"
  exit 0
else
  exit 1
fi