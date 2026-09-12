"""Packaging tests: generator git paths + manifest finish-args.

Ported from Gosh-Yubico-Authenticator-for-Linux/tests/test_packaging.py (D5:
T0/T2/T4 are ports, not greenfield). Selective port per packaging.md §2.5.2:
the generator tests (fixture cases, parse_git_source, sidecar shape, --check
staleness) and the finish-args guards. NOT ported: that app's YAML-manifest
tests (pinned toolchain archive, licence bundling, trademark notice, cosmic
icon-theme bundling, build-flatpak.sh runtime match, deb/rpm GTK tracking) —
ours is a JSON manifest on the rust-stable SDK extension with no build script
and no licence-bundling step.

Runs on stdlib only (no pytest/pyyaml): `python3 tests/test_packaging.py`.
verify.sh runs this file; CI job 2 runs it too. Keep both wired — the sibling
shipped T7 without a manifest arg because only one runner executed the guard.
"""

from __future__ import annotations

import importlib.util
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).parents[1]
MANIFEST = ROOT / "flatpak" / "io.github.gosh_distrobox_manager.json"
GENERATOR = ROOT / "flatpak" / "generate-cargo-sources.py"
CARGO_SOURCES = ROOT / "flatpak" / "cargo-sources.json"
GIT_PACKAGES = ROOT / "flatpak" / "git-packages.json"
GIT_MANIFESTS = ROOT / "flatpak" / "git-manifests"
FIXTURE = ROOT / "tests" / "fixtures" / "git-deps"


def _generator():
    spec = importlib.util.spec_from_file_location("generate_cargo_sources", GENERATOR)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


# The live lockfile HAS git sources now (11 remotes since T3), so unlike the
# sibling's "live has none until T6" both cases exercise the git path. The
# fixture still matters: it is the only input whose git set is small enough to
# assert exact emitted counts against, and it pins the rev/tag/unpinned shapes
# even if the live graph later moves.
CASES = [
    ("live", ROOT / "Cargo.lock", GIT_PACKAGES, GIT_MANIFESTS),
    ("fixture", FIXTURE / "Cargo.lock", FIXTURE / "git-packages.json",
     FIXTURE / "git-manifests"),
]


def _lockfile_git_sources(lockfile) -> set[str]:
    gen = _generator()
    packages = gen.parse_lockfile(lockfile)
    return {p["source"] for p in packages if p.get("source", "").startswith("git+")}


def test_fixture_actually_exercises_the_git_paths() -> None:
    """Guards the guard: the count assertions below are exact, so a fixture
    that silently lost a remote would fail loudly — but a fixture that lost
    ALL git sources would make several tests vacuous. This pins the floor."""
    sources = _lockfile_git_sources(FIXTURE / "Cargo.lock")
    assert len(sources) == 4, f"fixture must cover four git remotes, got {len(sources)}"
    qualifiers = {_generator().parse_git_source(s)["qualifier_kind"] for s in sources}
    assert {"rev", "tag", ""} <= qualifiers, (
        f"fixture must cover rev-pinned, tag-pinned and unpinned sources, got {qualifiers}"
    )


def test_generated_sources_are_current() -> None:
    """verify.sh runs this same --check; keep it green from the suite too."""
    result = subprocess.run(
        [sys.executable, str(GENERATOR), "--check"], capture_output=True, text=True
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_fixture_lockfile_generates_cleanly() -> None:
    """End-to-end over the fixture: lockfile + sidecar + manifests -> sources.

    Catches a sidecar that is stale or malformed in a way the key-set
    comparison cannot see. Counts recomputed from OUR fixture (4 remotes, 6
    packages) — not inherited from the sibling's 3-remote/7-package shape.
    """
    gen = _generator()
    sources = gen.generate(
        gen.parse_lockfile(FIXTURE / "Cargo.lock"),
        json.loads((FIXTURE / "git-packages.json").read_text()),
        FIXTURE / "git-manifests",
    )
    kinds = [s["type"] for s in sources]
    assert kinds.count("git") == 4, "one git source per remote"
    assert kinds.count("shell") == 4, "one flattening step per remote"
    # Each package gets a normalised Cargo.toml and a .cargo-checksum.json stub.
    assert kinds.count("inline") == 6 * 2 + 1, (
        "two inline sources per package, plus the cargo config"
    )


def test_git_sources_are_pinned_by_commit_never_a_branch() -> None:
    """A tag or branch is a movable ref; pinning to one breaks reproducibility."""
    gen = _generator()
    emitted = json.loads(CARGO_SOURCES.read_text())
    for _label, lockfile, sidecar, manifests in CASES:
        if (lockfile, sidecar) == (ROOT / "Cargo.lock", GIT_PACKAGES):
            continue
        emitted = emitted + gen.generate(
            gen.parse_lockfile(lockfile), json.loads(sidecar.read_text()), manifests
        )
    git_sources = [s for s in emitted if s.get("type") == "git"]
    assert git_sources, "no git sources emitted -- the fixture is not being exercised"
    for source in git_sources:
        assert "branch" not in source, f"branch-tracking git source: {source}"
        assert "tag" not in source, f"tag-tracking git source: {source}"
        commit = source.get("commit", "")
        assert len(commit) == 40 and all(
            c in "0123456789abcdef" for c in commit
        ), f"git source is not pinned to a full commit sha: {source}"


def test_git_sidecar_covers_exactly_the_lockfile_git_packages() -> None:
    """A stale sidecar must fail loudly rather than silently omit a dependency."""
    for label, lockfile, sidecar, _ in CASES:
        recorded = set(json.loads(sidecar.read_text()))
        assert recorded == _lockfile_git_sources(lockfile), f"mismatch in {label} tree"


def test_every_git_package_has_a_normalised_manifest() -> None:
    """Vendored packages are parsed standalone, so workspace inheritance must
    already be resolved -- see harvest_manifests in the generator."""
    checked = 0
    for label, _, sidecar_path, manifests in CASES:
        for entry in json.loads(sidecar_path.read_text()).values():
            for key in entry["packages"]:
                name, version = key.rsplit(" ", 1)
                manifest = manifests / f"{name}-{version}" / "Cargo.toml"
                assert manifest.is_file(), f"missing normalised manifest for {key} ({label})"
                assert ".workspace = true" not in manifest.read_text(), (
                    f"{key} manifest still inherits from a workspace root ({label})"
                )
                checked += 1
    assert checked, "no manifests checked -- the fixture is not being exercised"


def test_parse_git_source_takes_the_commit_from_the_fragment() -> None:
    gen = _generator()

    tagged = gen.parse_git_source(
        "git+https://github.com/pop-os/winit.git?tag=cosmic-0.14#71ce08c0" + "4" * 32
    )
    assert tagged["commit"] == "71ce08c0" + "4" * 32
    assert tagged["qualifier_kind"] == "tag"
    assert tagged["url"] == "https://github.com/pop-os/winit.git"

    unpinned = gen.parse_git_source(
        "git+https://github.com/pop-os/freedesktop-icons#ab4c57b8" + "e" * 32
    )
    assert unpinned["commit"] == "ab4c57b8" + "e" * 32
    assert unpinned["qualifier_kind"] == ""

    revved = gen.parse_git_source(
        "git+https://github.com/pop-os/libcosmic?rev=" + "7" * 40 + "#" + "7" * 40
    )
    assert revved["qualifier_kind"] == "rev"


def test_git_source_without_a_commit_is_rejected() -> None:
    gen = _generator()
    try:
        gen.parse_git_source("git+https://github.com/pop-os/libcosmic?branch=master")
    except SystemExit as exc:
        assert "locked commit" in str(exc)
    else:
        raise AssertionError("a git source with no locked commit must be rejected")


def test_git_sidecar_records_no_pins_of_its_own() -> None:
    """Cargo.lock must stay the only place a git commit is pinned (R8)."""
    sidecar = {}
    for _, _, sidecar_path, _ in CASES:
        sidecar.update(json.loads(sidecar_path.read_text()))
    assert sidecar, "no sidecar entries checked -- the fixture is not being exercised"
    for source, entry in sidecar.items():
        assert set(entry) == {"packages"}, f"{source} carries fields beyond packages: {entry}"
        for key, package in entry["packages"].items():
            assert set(package) == {"path", "entries"}, f"{key}: unexpected fields {package}"


def test_check_fails_when_the_sidecar_is_stale() -> None:
    """--check must reject a sidecar that disagrees with the lockfile."""
    gen = _generator()
    packages = gen.parse_lockfile(ROOT / "Cargo.lock")
    try:
        gen.generate(packages, {}, GIT_MANIFESTS)
    except SystemExit as exc:
        assert "--refresh-git" in str(exc)
    else:
        raise AssertionError("a lockfile git source with no sidecar entry must fail")


# --- manifest content: finish-args ----------------------------------------------

# Every permission the app ships with, and why. Changing this set is a
# deliberate act: add the arg here and in the manifest, in the same commit,
# with the reason. Set equality, not a subset check -- a subset would let a
# future --share=network or --device=all in silently.
REVIEWED_FINISH_ARGS = {
    "--share=ipc",                        # shared-memory transport for the toolkit
    "--socket=wayland",                   # primary display path (COSMIC session)
    "--socket=fallback-x11",              # X11 only when no Wayland compositor
    "--device=dri",                       # GPU for wgpu rendering
    "--talk-name=org.freedesktop.Flatpak",  # LOAD-BEARING: flatpak-spawn --host
    "--talk-name=org.a11y.Bus",           # LOAD-BEARING for the P0 a11y claim
    "--filesystem=xdg-config/cosmic:rw",  # cosmic-config persistence (:rw, not
                                          # the sibling's :ro — we write, D12)
}


def _finish_args() -> list[str]:
    """Parse finish-args from the JSON manifest without third-party deps."""
    return list(json.loads(MANIFEST.read_text())["finish-args"])


def _documented_finish_args() -> set[str]:
    """The finish-args set from docs/migration/packaging.md §1.4.

    Deliberately parsed from the doc rather than the manifest: a test that
    reads the manifest to build its expectation can only confirm the manifest
    agrees with itself (the sibling shipped a missing arg exactly this way).
    An independent source is the whole point.
    """
    lines = (ROOT / "docs" / "migration" / "packaging.md").read_text().splitlines()
    start = next(
        i for i, line in enumerate(lines) if line.strip() == '"finish-args": ['
    )
    args = set()
    for line in lines[start + 1:]:
        stripped = line.strip()
        if stripped == "]":
            break
        if stripped.startswith('"--'):
            args.add(stripped.strip('",'))
    return args


def test_manifest_matches_the_documented_recommendation() -> None:
    """Catches doc-vs-manifest drift in either direction."""
    documented = _documented_finish_args()
    assert documented, "could not parse the finish-args set from the doc"
    assert documented == REVIEWED_FINISH_ARGS, (
        f"doc says {sorted(documented)}\ntest says {sorted(REVIEWED_FINISH_ARGS)}"
    )


def test_finish_args_are_exactly_the_reviewed_set() -> None:
    actual = set(_finish_args())
    assert actual == REVIEWED_FINISH_ARGS, (
        f"added: {sorted(actual - REVIEWED_FINISH_ARGS)}  "
        f"removed: {sorted(REVIEWED_FINISH_ARGS - actual)}"
    )


def test_flatpak_spawn_grant_is_present() -> None:
    """Named separately so the failure explains itself: without
    --talk-name=org.freedesktop.Flatpak every distrobox/podman command fails
    and the app is non-functional under Flatpak (packaging.md §1.4)."""
    assert "--talk-name=org.freedesktop.Flatpak" in _finish_args()


def test_accessibility_talk_name_is_granted() -> None:
    """Named separately so the failure explains itself: without
    --talk-name=org.a11y.Bus accesskit never publishes a tree and Orca finds
    an app with no accessibility surface (REVIEW E2/UX-16)."""
    assert "--talk-name=org.a11y.Bus" in _finish_args()


def test_no_network_and_no_broad_device_access() -> None:
    """Redundant with the set-equality test, and deliberately so: these two
    are the over-grant regressions most likely to slip in (§1.4.1, §5.2)."""
    args = _finish_args()
    assert "--share=network" not in args
    assert "--device=all" not in args


def test_manifest_uses_the_freedesktop_runtime() -> None:
    manifest = json.loads(MANIFEST.read_text())
    assert manifest["runtime"] == "org.freedesktop.Platform"
    assert manifest["sdk"] == "org.freedesktop.Sdk"
    assert manifest["runtime-version"] == "25.08"
    assert manifest["base"] == "com.system76.Cosmic.BaseApp"
    assert "org.gnome." not in MANIFEST.read_text()


def test_the_flatpak_builds_the_only_binary_there_is() -> None:
    """The manifest's build line has no --bin flag, so it builds every bin —
    assert there is exactly one, named per D15. Cargo auto-discovers
    app/src/bin/*.rs on top of the declared [[bin]], so count files on disk,
    not tables in the manifest (the sibling learned this as D135)."""
    text = (ROOT / "app" / "Cargo.toml").read_text()
    # Parse, don't grep: the [package] name line also contains the string.
    in_bin = in_package = False
    bins, package_name = [], None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_bin = stripped == "[[bin]]"
            in_package = stripped == "[package]"
        elif in_bin and stripped.startswith("name = "):
            bins.append(stripped)
        elif in_package and stripped.startswith("name = "):
            package_name = stripped
    assert package_name and "gosh_distrobox_manager" in package_name
    assert len(bins) == 1 and "gosh_distrobox_manager" in bins[0], (
        f"expected exactly one [[bin]], got {bins}"
    )
    # Cargo auto-discovers app/src/bin/*.rs AND app/src/bin/*/main.rs on top
    # of declared targets — count files on disk, not tables (D135 class).
    bindir = ROOT / "app" / "src" / "bin"
    stray = []
    if bindir.is_dir():
        stray = list(bindir.glob("*.rs")) + [
            p for p in bindir.glob("*/main.rs") if p.parent.name != "bin"
        ]
    assert not stray, (
        f"app/src/bin/ auto-discovers into extra binaries: "
        f"{[p.name for p in stray]}. Either remove them or scope the "
        "manifest build line with --bin."
    )
    build = next(
        line for line in MANIFEST.read_text().splitlines()
        if "cargo --offline build" in line
    )
    assert "--locked" in build, f"offline reproducibility needs --locked: {build.strip()}"


# --- CI workflows: release artifact + single-sourced gate (T21/I20) ---------------
#
# stdlib text parsing, not a YAML library (this file runs on stdlib only):
# each assertion matches a distinctive literal the workflow must contain, and
# names the failure after the guarantee that broke. No ad-hoc grep as evidence
# (review O3) — these tests ARE the evidence, and they run in stage 8.

WORKFLOWS = ROOT / ".github" / "workflows"
FLATPAK_YML = WORKFLOWS / "flatpak.yml"
RUST_YML = WORKFLOWS / "rust.yml"
VERIFY_SH = ROOT / "scripts" / "verify.sh"
PACKAGING_MD = ROOT / "docs" / "migration" / "packaging.md"

APP_ID = "io.github.gosh_distrobox_manager"
TAG_GATE = "startsWith(github.ref, 'refs/tags/')"


def _expected_stages() -> int:
    """The single source of the stage count: EXPECTED_STAGES in verify.sh."""
    for line in VERIFY_SH.read_text().splitlines():
        if line.startswith("EXPECTED_STAGES="):
            return int(line.split("=", 1)[1])
    raise AssertionError("verify.sh defines no EXPECTED_STAGES")


def test_ci_flatpak_job_still_triggers_on_tags() -> None:
    """I20 arm (b) was rejected: the tags trigger must stay. Deleting it would
    remove the only shippable-artifact certifier (D19 gates the release on T3)."""
    text = FLATPAK_YML.read_text()
    assert "tags: [ v*.*.* ]" in text, "tags trigger missing from flatpak.yml"
    assert "workflow_dispatch:" in text, "workflow_dispatch trigger missing"


def test_ci_flatpak_job_exports_a_bundle_from_the_retained_repo() -> None:
    """I20 arm (a), first half: a tag-gated step bundles from the OSTree repo
    that verify.sh stage 9 retains."""
    text = FLATPAK_YML.read_text()
    assert "build-bundle .flatpak-builder/repo" in text, (
        "no build-bundle step exporting from the retained repo"
    )
    assert APP_ID in text, "bundle step names no app id"
    assert TAG_GATE in text, "bundle step is not gated on tags"


def test_ci_flatpak_job_attaches_the_bundle_to_the_release() -> None:
    """I20 arm (a), second half: the bundle lands on the GitHub Release as an
    asset. Run storage is explicitly NOT the mechanism (review O1)."""
    text = FLATPAK_YML.read_text()
    assert "action-gh-release" in text or "gh release upload" in text, (
        "no release-asset attach step in flatpak.yml"
    )
    assert ".flatpak" in text, "no .flatpak bundle file is attached"
    code = "\n".join(
        line for line in text.splitlines()
        if line.strip() and not line.strip().startswith("#")
    )
    assert "actions/upload-artifact" not in code, (
        "run storage is not release assets — must not be the publish path"
    )


def test_ci_flatpak_job_can_write_release_contents() -> None:
    """Attaching to a Release needs `contents: write`, scoped to the job —
    the workflow default must stay read."""
    text = FLATPAK_YML.read_text()
    assert "contents: write" in text, "flatpak job cannot write the release"
    assert "contents: read" in text, "workflow default permission must stay read"


def test_ci_rust_job_calls_verify_sh() -> None:
    """rust.yml must call the one script, not re-spell fmt/build/clippy/test
    (AGENTS.md: CI runs the same script)."""
    text = RUST_YML.read_text()
    assert "./scripts/verify.sh --fast" in text, "rust.yml does not call verify.sh"
    for respelled in ("run: cargo fmt", "run: cargo build",
                      "run: cargo clippy", "run: cargo test"):
        assert respelled not in text, f"rust.yml re-spells the gate: {respelled}"


def test_stage_count_is_single_sourced() -> None:
    """EXPECTED_STAGES agrees with the script's own `stage N` call sites (which
    must cover exactly 1..N), the workflow comments, and packaging.md §2.1 —
    and nowhere still says the old count."""
    expected = _expected_stages()
    numbers = sorted(
        int(line.split()[1])
        for line in VERIFY_SH.read_text().splitlines()
        if line.startswith("stage ") and line.split()[1].rstrip().isdigit()
    )
    assert numbers == list(range(1, expected + 1)), (
        f"verify.sh stages {numbers} do not cover exactly 1..{expected}"
    )
    assert f"ALL $EXPECTED_STAGES STAGES PASSED" in VERIFY_SH.read_text(), (
        "final banner must be built from EXPECTED_STAGES"
    )
    for workflow in (FLATPAK_YML, RUST_YML):
        assert f"1-{expected}" in workflow.read_text(), (
            f"{workflow.name} comment disagrees with EXPECTED_STAGES={expected}"
        )
    for workflow in WORKFLOWS.glob("*.yml"):
        assert "1-10" not in workflow.read_text(), (
            f"stale stage count in {workflow.name}"
        )
    assert f"grew from 7 stages to **{expected}**" in PACKAGING_MD.read_text(), (
        "packaging.md §2.1 disagrees with EXPECTED_STAGES"
    )


def test_verify_stage9_retains_an_ostree_repo() -> None:
    """Without --repo there is nothing for `flatpak build-bundle` to bundle
    from; the repo must live under .flatpak-builder/ (self-ignored)."""
    text = VERIFY_SH.read_text()
    assert "--repo=.flatpak-builder/repo" in text, (
        "stage 9 retains no OSTree repo for the tag job to bundle"
    )


def test_verify_preflights_validators_and_baseapp() -> None:
    """Stage-6 tools and the BaseApp must fail with a named cause in the
    preflight, not with a confusing mid-gate error."""
    text = VERIFY_SH.read_text()
    for tool in ("desktop-file-validate", "appstreamcli"):
        assert f"command -v {tool}" in text, f"no preflight for {tool}"
    assert "com.system76.Cosmic.BaseApp" in text, "no preflight for the BaseApp"


def main() -> int:
    tests = sorted(
        (name, fn) for name, fn in globals().items()
        if name.startswith("test_") and callable(fn)
    )
    failed = 0
    for name, fn in tests:
        try:
            fn()
        except AssertionError as exc:
            failed += 1
            print(f"FAIL {name}: {exc}")
        except Exception as exc:  # noqa: BLE001 — report, don't hide
            failed += 1
            print(f"ERROR {name}: {type(exc).__name__}: {exc}")
        else:
            print(f"ok {name}")
    print(f"{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
