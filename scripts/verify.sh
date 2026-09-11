#!/usr/bin/env bash
# verify.sh — the single gate script for the migration (D13).
#
# Stages, cheapest first (packaging.md §2.1): fmt, build, clippy, test,
# versions, metadata, generator --check, packaging tests, flatpak build,
# smoke test. Fail-fast (set -euo pipefail); each stage prints a banner so a
# failure is attributable without reading the whole log.
#
# --locked lives here (CI + release), NOT in the local loop (D13/PKG-11):
# running verify.sh with a stale lockfile must fail loudly. Local iteration
# uses plain cargo commands without --locked.
#
# Usage:
#   ./scripts/verify.sh              # everything, incl. flatpak build + smoke
#   ./scripts/verify.sh --fast       # stages 1-8 only (no flatpak, no network
#                                    # beyond the cargo cache) — the per-push gate
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

FAST_ONLY=0
if [ "${1:-}" = "--fast" ]; then
    FAST_ONLY=1
fi

stage() {
    echo "===== verify.sh stage $1: $2 ====="
}

# --- preflight (flatpak stages only) ------------------------------------------
if [ "$FAST_ONLY" -eq 0 ]; then
    command -v flatpak-builder >/dev/null || { echo "flatpak-builder not found" >&2; exit 1; }
    flatpak remotes --user 2>/dev/null | grep -q '^flathub' || {
        echo "verify.sh: FATAL — flathub remote missing (user)" >&2; exit 1; }
    flatpak info --user org.freedesktop.Sdk//25.08 >/dev/null 2>&1 || {
        echo "verify.sh: FATAL — org.freedesktop.Sdk 25.08 not installed" >&2; exit 1; }
fi

# ---- 1. Formatting ------------------------------------------------------------
stage 1 "cargo fmt --check"
cargo fmt --all -- --check

# ---- 2. Build (release, as the flatpak will) ----------------------------------
stage 2 "cargo build --workspace --release --locked"
cargo build --workspace --release --locked

# ---- 3. Lint — warnings are errors --------------------------------------------
stage 3 "cargo clippy --workspace --all-targets --locked"
cargo clippy --workspace --all-targets --locked -- -D warnings

# ---- 4. Unit + integration tests ----------------------------------------------
stage 4 "cargo test --workspace --locked"
cargo test --workspace --locked

# ---- 5. Version unification (D14) ----------------------------------------------
stage 5 "scripts/check-versions.sh"
./scripts/check-versions.sh

# ---- 6. Packaging metadata validation ------------------------------------------
stage 6 "desktop-file-validate + appstreamcli"
desktop-file-validate core/data/io.github.gosh_distrobox_manager.desktop
appstreamcli validate --no-net core/data/io.github.gosh_distrobox_manager.metainfo.xml

# ---- 7. Vendoring staleness gate (T2/D26, offline) ------------------------------
stage 7 "generate-cargo-sources.py --check"
python3 flatpak/generate-cargo-sources.py --check

# ---- 8. Packaging tests (T4/D26: stdlib only, no pytest) -------------------------
stage 8 "tests/test_packaging.py"
python3 tests/test_packaging.py

if [ "$FAST_ONLY" -eq 1 ]; then
    echo "verify.sh --fast: STAGES 1-8 PASSED"
    exit 0
fi

# ---- 9. Flatpak build (offline; installs for the smoke test) ---------------------
# --user --install (D26d): stage 6-as-sketched with --repo alone never
# installs, and the smoke test's `flatpak run` needs an installed app.
# --state-dir pins the tool's working files where its own .gitignore covers
# them; --repo likewise stays out of the tree (D26b).
stage 9 "flatpak-builder offline build + install"
flatpak-builder --user --install --force-clean --disable-rofiles-fuse \
    --state-dir=.flatpak-builder/state \
    .flatpak-builder/build \
    flatpak/io.github.gosh_distrobox_manager.json

# ---- 10. Smoke test (D17: alive + readiness + clean SIGTERM + negative) ---------
stage 10 "smoke-test.sh (positive)"
./scripts/smoke-test.sh

stage 10 "smoke-test.sh (negative: no flatpak-spawn grant)"
./scripts/smoke-test.sh --negative-flatpak-spawn

echo "verify.sh: ALL STAGES PASSED"
