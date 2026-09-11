#!/usr/bin/env bash
# smoke-test.sh — launch the built Flatpak, assert it does something, exit clean.
#
# D17 (mandatory, all four): (1) process stays alive; (2) readiness line on
# first completed render; (3) clean SIGTERM exit code; (4) negative case —
# without --talk-name=org.freedesktop.Flatpak the backend escape hatch is
# gone and operations must fail (proves the grant is load-bearing, §1.4).
# Widget-presence hook deferred and recorded (packaging.md Q10 / D17).
#
# Display strategy (packaging.md §2.4): a live session (local COSMIC) is used
# directly; a headless runner wraps itself in xvfb-run; neither available is
# a hard failure, not a skip. In this repo the binary refuses headless
# startup without a display (winit has nothing to bind), so "no display" can
# never be papered over by asserting on a process that never rendered.
set -euo pipefail

APP_ID="${SMOKE_APP_ID:-io.github.gosh_distrobox_manager}"
STAY_ALIVE_SECONDS="${SMOKE_STAY_ALIVE:-10}"
READY_TIMEOUT="${SMOKE_READY_TIMEOUT:-60}"
# Scratch files: the captured app log; the trap cleans both even on FAIL.
LOG_FILE="$(mktemp)"
trap 'rm -f "$LOG_FILE"' EXIT

# --- display strategy ---------------------------------------------------------
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    if command -v xvfb-run >/dev/null 2>&1; then
        echo "smoke: no display; re-execing under xvfb-run (software rendering)" >&2
        exec xvfb-run -a --server-args="-screen 0 1280x800x24" \
            env LIBGL_ALWAYS_SOFTWARE=1 "$0" "$@"
    fi
    echo "smoke: FATAL — no display and no xvfb-run available" >&2
    exit 1
fi

mode="positive"
if [ "${1:-}" = "--negative-flatpak-spawn" ]; then
    mode="negative"
fi

# --- launch -------------------------------------------------------------------
# Explicit --command invocation (same sandbox, same finish-args: `flatpak run
# "$APP_ID"` execs the identical binary). --command is NOT required for the
# redirect — the advocate measured plain `flatpak run "$APP_ID" &` filling it
# with 2571 bytes incl. SMOKE_READY (flatpak 1.18.2); the earlier "stays
# empty" reading was an artefact of asserting on the journal instead of the
# file. Kept for explicitness: the command line states exactly what runs.
RUN_ARGS=()
if [ "$mode" = "negative" ]; then
    # D17 (4): drop the Flatpak portal grant. --no-talk-name overrides the
    # manifest's finish-args for this launch only; the installed app is
    # untouched.
    RUN_ARGS+=(--no-talk-name=org.freedesktop.Flatpak)
fi
flatpak run "${RUN_ARGS[@]}" --command=sh "$APP_ID" \
    -c 'exec gosh_distrobox_manager' >"$LOG_FILE" 2>&1 &
APP_PID=$!

# --- (1) stay-alive ------------------------------------------------------------
sleep "$STAY_ALIVE_SECONDS"
if ! kill -0 "$APP_PID" 2>/dev/null; then
    wait "$APP_PID" || true
    echo "smoke ($mode): FAIL — process exited within ${STAY_ALIVE_SECONDS}s" >&2
    exit 1
fi
echo "smoke ($mode): process alive after ${STAY_ALIVE_SECONDS}s"

# --- (2) readiness --------------------------------------------------------------
# The binary logs SMOKE_READY once its first render completes (see
# app/src/app.rs `view`). Liveness alone passes for a blank frozen window;
# the line proves the app actually rendered. Read from the captured log
# file — NOT journalctl: `flatpak run &` detaches the sandbox's stdout into
# journal forwarding that `journalctl --user` never shows (measured
# 2026-09-11), while a foreground `flatpak run` streams it to our pipe.
deadline=$((SECONDS + READY_TIMEOUT))
ready=0
while [ "$SECONDS" -lt "$deadline" ]; do
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        echo "smoke ($mode): FAIL — died while waiting for readiness" >&2
        wait "$APP_PID" || true
        exit 1
    fi
    if grep -q "SMOKE_READY" "$LOG_FILE" 2>/dev/null; then
        ready=1
        break
    fi
    sleep 2
done
if [ "$mode" = "positive" ] && [ "$ready" -ne 1 ]; then
    echo "smoke ($mode): FAIL — no SMOKE_READY within ${READY_TIMEOUT}s" >&2
    echo "smoke ($mode): --- captured log head ---" >&2
    head -30 "$LOG_FILE" >&2 || true
    kill -TERM "$APP_PID" || true
    wait "$APP_PID" || true
    exit 1
fi
[ "$ready" -eq 1 ] && echo "smoke ($mode): readiness line observed"

# --- (2b) negative-case backend probe (D17 (4)) -----------------------------------
# Dropping --talk-name=org.freedesktop.Flatpak must make host operations fail:
# without the grant, `flatpak-spawn --host` cannot escape the sandbox, so a
# `distrobox --version` probe through the portal must error. A negative run
# that renders fine (readiness above) but whose probe SUCCEEDS proves nothing
# — the grant would not be load-bearing. Probe via `flatpak-spawn --host`
# from INSIDE the sandbox (--command=sh), so the test exercises the exact
# portal path the Backend uses (core/src/env.rs detect → CommandRunner).
if [ "$mode" = "negative" ]; then
    # NOTE: assert on the INNER exit code, echoed to stdout: 1 when the
    # portal refuses. (The outer `flatpak run` exit is 0 here only because
    # the -c string ends with `echo $?`; a bare failing command propagates
    # outer 1 — measured 2026-09-11. Either way the outer code is not the
    # signal; the inner 1 vs 0 is.)
    INNER="$(flatpak run --no-talk-name=org.freedesktop.Flatpak --command=sh "$APP_ID" \
        -c 'flatpak-spawn --host true >/dev/null 2>&1; echo $?' 2>/dev/null)"
    if [ "$INNER" != "1" ]; then
        echo "smoke (negative): FAIL — host spawn not refused without the grant (inner=$INNER)" >&2
        echo "smoke (negative): --talk-name=org.freedesktop.Flatpak is NOT load-bearing" >&2
        kill -TERM "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" || true
        exit 1
    fi
    echo "smoke (negative): host spawn refused without the grant (load-bearing confirmed)"
fi

# --- (3) clean exit ---------------------------------------------------------------
# Signal the APP, not the `flatpak run` wrapper: SIGTERM to the wrapper kills
# the supervisor with 143 while the sandboxed binary keeps running (measured
# 2026-09-11). The binary's own PID is signalled instead — it exits on TERM
# (winit/iced shutdown path) — then the wrapper is reaped. `flatpak kill`
# takes no --signal flag in flatpak 1.18 (SIGKILL only), so direct signalling
# is the only TERM path. The pgrep pattern uses the [r] trick so it never
# matches this script's own cmdline (the D26 self-match class).
BIN_PID="$(pgrep -f '^gosh_distrobox_manage[r]' | head -1 || true)"
if [ -z "$BIN_PID" ]; then
    echo "smoke ($mode): FAIL — no app binary found to signal" >&2
    kill -TERM "$APP_PID" || true
    wait "$APP_PID" || true
    exit 1
fi
kill -TERM "$BIN_PID"
deadline=$((SECONDS + 15))
while kill -0 "$BIN_PID" 2>/dev/null && [ "$SECONDS" -lt "$deadline" ]; do
    sleep 1
done
if kill -0 "$BIN_PID" 2>/dev/null; then
    echo "smoke ($mode): FAIL — binary still alive 15s after SIGTERM" >&2
    exit 1
fi
kill -TERM "$APP_PID" 2>/dev/null || true
wait "$APP_PID" || true
if pgrep -f '^gosh_distrobox_manage[r]' >/dev/null 2>&1; then
    echo "smoke ($mode): FAIL — binary lingers after SIGTERM" >&2
    exit 1
fi
echo "smoke ($mode): PASS — clean exit on SIGTERM"
