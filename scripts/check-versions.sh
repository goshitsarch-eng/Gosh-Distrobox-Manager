#!/usr/bin/env bash
# check-versions.sh — fail when the release version drifts between artifacts.
#
# D14: core/Cargo.toml `package.version` is the single source of truth.
# Everything else must equal it. This gates scripts/verify.sh (and CI job 2),
# turning the recurring 1.0.0-vs-1.0.2 drift class into an automated failure
# instead of a review comment.
#
# Checked: core/Cargo.toml, app/Cargo.toml, Cargo.lock (both crates),
# gosh-distrobox-manager.spec (Version: + %changelog entry), build-rpm.sh
# (VERSION=), RPM-BUILD.md (the filename examples + the Version bullet),
# core/data/io.github.gosh_distrobox_manager.metainfo.xml (newest <release>).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail=0
report() {
    if [ "$2" = "$EXPECTED" ]; then
        echo "check-versions: OK   $1 = $2"
    else
        echo "check-versions: FAIL $1 = $2 (expected $EXPECTED)" >&2
        fail=1
    fi
}

# --- source of truth ----------------------------------------------------------
EXPECTED="$(sed -n 's/^version = "\(.*\)"/\1/p' core/Cargo.toml | head -1)"
if [ -z "$EXPECTED" ]; then
    echo "check-versions: FAIL could not read version from core/Cargo.toml" >&2
    exit 1
fi
echo "check-versions: source of truth core/Cargo.toml = $EXPECTED"

# --- Cargo side ---------------------------------------------------------------
report "app/Cargo.toml" "$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -1)"

for crate in gosh-distrobox-core gosh_distrobox_manager; do
    v="$(awk -v name="$crate" '
        /^\[\[package\]\]/ { want = 0 }
        $0 == "name = \"" name "\"" { want = 1 }
        want && /^version = / { gsub(/"/, ""); print $3; exit }
    ' Cargo.lock)"
    report "Cargo.lock ($crate)" "$v"
done

# --- RPM side -----------------------------------------------------------------
report "gosh-distrobox-manager.spec Version:" \
    "$(sed -n 's/^Version:[[:space:]]*//p' gosh-distrobox-manager.spec | head -1)"
report "build-rpm.sh VERSION" \
    "$(sed -n 's/^VERSION="\(.*\)"/\1/p' build-rpm.sh | head -1)"

# Exact-match (not prefix): EXPECTED=1.0.2 must not match a 1.0.20 bullet.
# Every correct occurrence is followed by `-` (filenames `1.0.2-1…`, changelog
# `1.0.2-1`), so require it literally — no regex needed. The `**Version**`
# bullet ends `1.0.2` at end-of-line (RPM-BUILD.md:58), checked separately.
if grep -q "gosh-distrobox-manager-${EXPECTED}-" RPM-BUILD.md \
    && grep -q -- "- \*\*Version\*\*: ${EXPECTED}\$" RPM-BUILD.md; then
    echo "check-versions: OK   RPM-BUILD.md references $EXPECTED"
else
    echo "check-versions: FAIL RPM-BUILD.md does not reference $EXPECTED consistently" >&2
    fail=1
fi

if grep -q -- "- ${EXPECTED}-" gosh-distrobox-manager.spec; then
    echo "check-versions: OK   spec %changelog mentions $EXPECTED"
else
    echo "check-versions: FAIL spec %changelog has no entry for $EXPECTED" >&2
    fail=1
fi

# --- metainfo side ------------------------------------------------------------
# Newest <release> = first version attribute in document order (the file lists
# newest first per the AppStream spec).
META_NEWEST="$(sed -n 's/.*<release version="\([^"]*\)".*/\1/p' \
    core/data/io.github.gosh_distrobox_manager.metainfo.xml | head -1)"
report "metainfo newest <release>" "$META_NEWEST"

if [ "$fail" -ne 0 ]; then
    echo "check-versions: VERSION DRIFT DETECTED" >&2
    exit 1
fi
echo "check-versions: ALL VERSIONS AGREE ($EXPECTED)"
