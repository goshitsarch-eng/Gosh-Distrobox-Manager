# English catalogue for Gosh Distrobox Manager.
#
# Conventions (T15):
# - `app-*` ids are chrome; per-screen ids are prefixed by screen
#   (`dashboard-`, `containers-`, `wizard-`, ...).
# - User data NEVER goes in a message body. Container names, image URLs,
#   distro names, package names and command output are placeables passed by the
#   caller ({ $name }), so a translator cannot break them and no user string is
#   ever re-interpreted as Fluent syntax.
# - `$count` messages use plural selectors, not "1 container(s)".

app-title = Gosh Distrobox Manager

# Navigation
nav-dashboard = Dashboard
nav-containers = Containers
nav-images = Images
nav-packages = Packages
nav-updates = Updates
nav-backups = Backups
nav-apps = Apps
nav-activity = Activity
nav-settings = Settings
# `Page::Stats` ("Stats" is the Flutter rail's own label; the page is stubbed).
nav-stats = Stats

# Generic actions
action-refresh = Refresh
action-retry = Retry
action-cancel = Cancel
action-close = Close
action-back = Back
action-start = Start
action-stop = Stop
action-remove = Remove
action-create = Create

# States
state-loading = Loading…

# ---------------------------------------------------------------------------
# Packages page (T8, ux.md §6.9)
# ---------------------------------------------------------------------------

packages-container-heading = CONTAINER
packages-status-running = Running
packages-status-stopped = Stopped
packages-detecting = Detecting…
packages-tab-installed = Installed
packages-tab-search = Search
packages-tab-search-results = Search Results
# (The row's Install/Remove buttons reuse the existing `app-install` and
# `action-remove` ids — same wording, same role.)
# Package version chip + description (`{ $version } — { $description }`); both
# are user data and never part of the message text.
packages-version-description = { $version } — { $description }
packages-not-running-warning = Container is not running. Start it to manage packages.
packages-manual-prompt = No supported package manager detected. Run a command manually:
packages-manual-placeholder = e.g. apt-get install foo
packages-manual-run = Run (lands in T9)
packages-loading = Loading packages…
packages-searching = Searching…
packages-error-title = Could not load packages
packages-empty-not-running-title = Container Not Running
packages-empty-not-running-body = Start the container to view installed packages
packages-empty-none-title = No packages found
packages-empty-search-title = Search for packages
packages-empty-search-body = Enter a package name and press Enter
packages-empty-no-match-body = Try a different search term
packages-count-installed = { $count ->
    [one] One package installed
   *[other] { $count } packages installed
  }
packages-count-found = { $count ->
    [one] One package found
   *[other] { $count } packages found
  }

# ---------------------------------------------------------------------------
# Apps export page (T12, ux.md §6.14)
# ---------------------------------------------------------------------------

# Title is `Apps — {container}`; the container name is user data.
apps-title = Apps — { $container }
apps-search-placeholder = Search apps...
apps-manual-binary-export = Manual Binary Export
apps-loading = Loading apps…
apps-error-title = Could not load apps
apps-section-installed = Installed in Container
apps-exported = EXPORTED
apps-not-exported = NOT EXPORTED
apps-empty-select-title = Select a container
apps-empty-select-body = Open a container's Applications tile to manage its exported apps.
apps-empty-no-apps-title = No applications found in this container
apps-empty-no-match-title = No applications match your search
apps-count-found = { $count ->
    [one] One Found
   *[other] { $count } Found
  }
apps-count-binaries = { $count ->
    [one] One exported binary
   *[other] { $count } exported binaries
  }
# Export-binary dialog (#179).
apps-binary-dialog-body = Enter the path to a binary inside the container to export it to your host system.
apps-binary-path-placeholder = /usr/bin/some-command

# Settings page (ux.md §6.13)
settings-system-information = SYSTEM INFORMATION
settings-distrobox-version = Distrobox Version
settings-total-containers = Total Containers
settings-running-containers = Running Containers
settings-distrobox-installed = Distrobox Installed
settings-yes = Yes
settings-no = No
settings-preferences = PREFERENCES
settings-confirm-destructive = Confirm destructive actions
settings-confirm-destructive-description = Ask before remove, stop-all, delete
settings-show-skipped-rows = Show skipped rows
settings-show-skipped-rows-description = Report container rows that could not be parsed
settings-snapshot-prefix = Snapshot Prefix
settings-snapshot-prefix-placeholder = gdm
settings-default-export-dir = Default Export Directory
settings-default-export-dir-placeholder = ~/Downloads
settings-about-comments = A GUI for managing Distrobox containers.
settings-link-source-code = Source Code
settings-link-distrobox-docs = Distrobox Docs
settings-danger-zone = DANGER ZONE
settings-delete-all-containers = Delete All Containers
settings-preferences-unavailable = Preferences unavailable
settings-preferences-unavailable-body = Settings will not persist this session.

# Create wizard — step 1 (image)
wizard-select-image-title = Select Image
wizard-select-image-body = Choose a Linux distribution for your container.
wizard-search-placeholder = Search distributions...
wizard-no-images = No images available
wizard-custom-url-heading = CUSTOM IMAGE URL
wizard-custom-url-placeholder = e.g. docker.io/library/ubuntu:22.04
wizard-next = Next

# Create wizard — step 2 (configuration)
wizard-config-title = Configuration
wizard-config-subtitle = Step 2 of 3: System settings
wizard-selected-image-heading = SELECTED IMAGE
wizard-container-name-label = Container Name
wizard-name-placeholder = e.g. arch-dev-box
wizard-init-system = Init System
wizard-init-system-desc = Run an init system inside the container
wizard-nvidia = NVIDIA GPU Support
wizard-nvidia-desc = Enable NVIDIA GPU passthrough
wizard-advanced-expanded = Advanced Options ▾
wizard-advanced-collapsed = Advanced Options ▸
wizard-home-dir-label = Home Directory
wizard-home-dir-placeholder = /home/user/containers/my-box
wizard-volumes-label = Volume Mounts
wizard-no-volumes = No volumes configured
# Volume row: host and container paths are user data ($host / $container).
wizard-volume-mount = { $host } -> { $container }
wizard-volume-mount-read-only = { $host } -> { $container } (Read-only)
wizard-volume-add = Add Volume

# Create wizard — step 3 (progress)
wizard-creating-title = Creating Container...
wizard-created-title = Container Created!
wizard-create-failed-title = Creation Failed
wizard-waiting-output = Waiting for output...
wizard-done = Done

# Add-volume dialog
wizard-volume-dialog-host = Host Path
wizard-volume-dialog-container = Container Path
wizard-volume-path-placeholder = /mnt/data
wizard-volume-dialog-read-only = Read-only

# Image cards (shared by the wizard grid and the Images page)
wizard-image-card-select = Select
wizard-image-card-selected = ✓ Selected
wizard-image-details = Details

# Activity log (nav-activity page)
activity-state-running = In Progress
activity-state-success = Completed
activity-state-failed = Failed

# Timeline row + output drawer metadata: "{ relative time } · { status }".
# Both values are placeables — the status is itself an already-localized
# activity-state-* message.
activity-timeline-meta = { $time } · { $state }
activity-output = Output
activity-no-output = No output yet.

# Relative time (row #162 ladder). $count is the elapsed minutes/hours/days;
# a Fluent plural selector replaces the bare "5m ago" / "3d ago" suffixes.
activity-time-just-now = Just now
activity-time-minutes = { $count ->
    [one] { $count }m ago
   *[other] { $count }m ago
  }
activity-time-hours = { $count ->
    [one] { $count }h ago
   *[other] { $count }h ago
  }
activity-time-days = { $count ->
    [one] { $count }d ago
   *[other] { $count }d ago
  }

# Search + filter chips
activity-search-placeholder = Search logs...
activity-filter-all = All
activity-filter-running = Running
activity-filter-success = Success
activity-filter-errors = Errors

# Stats bar (row #155)
activity-stat-total = Total
activity-stat-running = Running
activity-stat-completed = Completed
activity-stat-failed = Failed

# Empty states (row #156)
activity-empty-title = No activity yet
activity-empty-body = Your container operations will appear here
activity-empty-match-title = No matching activities
activity-empty-match-body = Try adjusting your search or filters

# ---------------------------------------------------------------------------
# Shell + app-level copy (unit "app", app/src/app.rs).
#
# These ids are things the shell owns rather than any single screen: the error
# banner, the header buttons, the shared confirm specs, and the TASK LABELS
# (`app-label-*`). These are user-visible and therefore TRANSLATED, so no code
# may match on their text. Routing that used to key off these labels (the
# Updates page's `Upgrade ` filter, the wizard's two `Create ` prefix tests)
# now goes through `TaskKind` instead — see `TaskView::kind` in app.rs. If you
# find a `starts_with` against one of these messages, it is a bug: it holds
# only in the fallback locale and breaks silently in every translation.
# ---------------------------------------------------------------------------

# Shell chrome
app-dismiss = Dismiss
app-quick-actions = QUICK ACTIONS
app-about-heading = ABOUT
app-refresh-all-data = Refresh All Data
app-stop-all-containers = Stop All Containers
app-upgrade-all-containers = Upgrade All Containers
app-clear-completed-tasks = Clear Completed Tasks
app-new-container = New Container
app-upgrade-all = Upgrade All
app-clear-completed = Clear completed
app-loading-containers = Loading containers…
app-create-first-container = Create your first container to get started.
app-version-unknown = Unknown

# Stats page
app-stats-select = Select a container to show its stats.
app-stats-loading = Loading stats for { $name }…
app-stats-none = No stats for { $name } yet.
app-stats-title = Stats: { $name }
app-stat-cpu = CPU: { $percent }%
app-stat-memory = Memory: { $used } / { $limit } ({ $percent }%)
app-stat-network = Network I/O: { $value }
app-stat-block = Block I/O: { $value }

# Shared error copy (backend failures the user reads in the banner or a toast)
app-blocked-environment = Running inside a Distrobox container without distrobox-host-exec. Install distrobox-host-exec on the host or run on the host system.
app-command-failed = `{ $command }` failed: { $stderr }
app-failed = Failed: { $error }
app-could-not-start = Could not start { $label }: { $error }
app-could-not-start-creation = Could not start creation: { $error }
app-task = Task
app-task-failed = { $label } failed — see output for details
app-search-failed = Search failed: { $error }
app-could-not-read-version = Could not read distrobox version: { $error }
app-could-not-open = Could not open { $url }: { $error }

# Container actions + their destructive confirms
app-delete-container = Delete Container
app-confirm-delete-container = Are you sure you want to delete "{ $name }"?\n\nThis action cannot be undone and all container data will be lost.
app-delete = Delete
app-delete-all-containers = Delete All Containers
app-confirm-delete-all-containers = Are you sure you want to delete ALL containers?\n\nThis action cannot be undone and all container data will be lost.
app-delete-all = Delete All
app-stop-all = Stop All
app-confirm-stop-all = { $count ->
    [one] Stop all { $count } running container?
   *[other] Stop all { $count } running containers?
  }
app-no-running-to-stop = No running containers to stop.
app-confirm-upgrade-all = { $count ->
    [one] Upgrade packages in { $count } running container?
   *[other] Upgrade packages in { $count } running containers?
  }
app-no-running-to-upgrade = No running containers to upgrade.
app-clone-name-invalid = Invalid container name "{ $name }": must match [a-zA-Z0-9][a-zA-Z0-9_.-]* (row #96).
app-cloning-container = Cloning { $source } to { $name }…
app-upgrading-container = Upgrading { $name }…
app-copied-to-clipboard = Copied { $what } to clipboard

# Short-mutation results (toasts)
app-container-deleted = { $name } deleted
app-container-stopped = { $name } stopped
app-container-started = { $name } started
app-all-containers-stopped = All containers stopped
app-all-containers-deleted = { $count ->
    [one] All { $count } container deleted
   *[other] All { $count } containers deleted
  }
app-containers-deleted-partial = { $total ->
    [one] Deleted { $done } of { $total } container — failed: { $failures }
   *[other] Deleted { $done } of { $total } containers — failed: { $failures }
  }

# Task labels (rendered in Dashboard/Activity rows and matched by prefix)
app-label-upgrade = Upgrade { $name }
app-label-create = Create { $name }
app-label-clone = Clone to { $name }
app-label-restore = Restore { $snapshot } to { $name }
app-label-export = Export { $container } to { $path }
app-label-import = Import { $path } as { $image }
app-label-install = Install { $package } in { $container }
app-label-remove-package = Remove { $package } from { $container }

# Images page
app-custom-image-url-first = Enter a custom image URL first.

# Apps page
app-export-binary = Export Binary
app-export = Export
app-binary-path-required = Binary path is required.
app-binary-exported = Binary exported: { $path }
app-application-exported = Application exported
app-application-unexported = Application unexported

# Create wizard
app-add-volume = Add Volume
app-add = Add
app-please-select-an-image = Please select an image
app-please-enter-container-name = Please enter a container name
app-invalid-container-name = Invalid container name: { $error }
app-host-path-required = Host path is required.
app-container-path-required = Container path is required.

# Terminal page
app-no-terminal-available = No terminal available to launch.
app-launched-terminal = Launched { $terminal } for { $container }
app-could-not-launch-terminal = Could not launch terminal: { $error }
app-launch-failed = Launch failed: { $error }

# Packages page
app-packages-no-containers = Create a container before managing packages.
app-search-packages = Search for packages...
app-clear-search = Clear search
app-package-manager = Package manager: { $manager }
app-install = Install
app-remove-package = Remove Package
app-confirm-remove-package = Remove "{ $package }" from "{ $container }"?\n\nThis may also remove dependent packages.
app-upgrade-all-packages = Upgrade All Packages
app-confirm-upgrade-packages = Upgrade all packages in "{ $container }"?
app-install-package = Install Package
app-confirm-install-package = Install "{ $package }" in "{ $container }"?
app-manual-commands-t9 = Manual commands run in T9 — container must be running.

# Backups page
app-backups-no-containers = Create a container before managing backups.
app-snapshots = Snapshots
app-new-snapshot = New Snapshot
app-export-import = Export / Import
app-select-container-first = Select a container first.
app-snapshot-created = Snapshot created
app-snapshot-deleted = Snapshot deleted
app-could-not-create-snapshot = Could not create snapshot: { $error }
app-could-not-delete-snapshot = Could not delete snapshot: { $error }
app-delete-snapshot = Delete Snapshot
app-confirm-delete-snapshot = Are you sure you want to delete "{ $name }"?\n\nThis action cannot be undone.
app-create-snapshot = Create Snapshot
app-create-snapshot-of = Create a snapshot of "{ $name }"
app-snapshot-name-placeholder = e.g. mybox-snapshot
app-snapshot-commit-helper = Snapshots are saved as container images using podman/docker commit.
app-snapshot-name-required = Snapshot name is required.
app-restore-from-snapshot = Restore from Snapshot
app-restore-create-container = Create a new container from "{ $snapshot }"
app-new-container-name-placeholder = New container name
app-new-container-name-required = New container name is required.
app-restore = Restore
app-export-container = Export Container
app-export-as-tar = Export "{ $name }" as a tar archive
app-export-path-placeholder = /tmp/mybox-export.tar
app-browse = Browse…
app-export-size-warning = The archive may be several gigabytes. Ensure the destination has space.
app-output-path-required = Output path is required.
app-import-container = Import Container
app-archive-path = Archive path
app-image-name = Image name
app-import-image-placeholder = mybox-imported
app-archive-and-image-required = Archive path and image name are required.
app-import = Import

# Shell-level results + portal file-chooser titles (emitted by app.rs)
app-data-refreshed = Data refreshed
app-completed-tasks-cleared = Completed tasks cleared
app-imported-settings = Imported settings from DistroShelf
app-settings-no-persist = Settings will not persist this session.
app-file-chooser-export-title = Export container
app-file-chooser-import-title = Import container archive

# Backups page — screen-specific copy (unit "backups", app/src/backups.rs).
#
# The dialog titles, confirms, toasts and placeholders above are `app-*` ids
# owned by app.rs; what follows is text only the Backup page's own bodies
# render. Snapshot `created`/`size` are backend output, so they are
# placeables: no user or command text ever enters a message body.
backup-snapshot-meta = { $created } · { $size }
backup-loading-snapshots = Loading snapshots…
backup-could-not-load = Could not load snapshots
backup-empty-no-snapshots = No snapshots yet
backup-empty-no-snapshots-body = Create your first snapshot to get started.
backup-create-first = Create First Snapshot

# Plural selector rather than "N snapshot(s)": a translator can then choose a
# form that does not read "One snapshot" as "1 snapshot".
backup-n-snapshots = { $count ->
    [one] One snapshot
   *[other] { $count } snapshots
  }

backup-export-heading = EXPORT CONTAINER
backup-export-description = Save a container to a tar archive (portal file chooser).
backup-export-action = Export…
backup-import-heading = IMPORT CONTAINER
backup-import-description = Restore a container from a tar archive (portal file chooser).
backup-import-action = Import…
backup-clone-heading = CLONE CONTAINER
backup-clone-description = Create a copy of the selected container.
backup-clone-action = Clone…

# ---------------------------------------------------------------------------
# Unit "misc": Terminal launch page (app/src/terminal.rs), Images page
# (app/src/images_view.rs), Updates page (app/src/updates.rs).
#
# Same rules as above. Container names, image URLs, the enter-argv command,
# status labels, snapshot/backend values and the details values are user data,
# so they stay placeables. `misc-terminal-details-row` takes BOTH halves as
# placeables because its key is itself a localized `misc-terminal-detail-*`
# message (same shape as `activity-timeline-meta`).
# ---------------------------------------------------------------------------

# Terminal launch page. `misc-upgrading` is shared with the Updates page —
# both render the same in-flight label.
misc-terminal-access-heading = TERMINAL ACCESS
misc-terminal-not-running-warning = Container is not running. Start the container to access the terminal.
misc-terminal-loading-command = Loading terminal command…
misc-terminal-command-error = Error: { $error }
misc-terminal-enter-lead = Run this command in your terminal to enter the container:
misc-terminal-copy-command = Copy Command
misc-terminal-picker-heading = TERMINAL
misc-terminal-launch = Launch Terminal
misc-terminal-stop-container = Stop Container
misc-terminal-upgrade-packages = Upgrade Packages
misc-upgrading = Upgrading…
misc-terminal-details-heading = CONTAINER DETAILS
misc-terminal-details-row = { $key }: { $value }
misc-terminal-detail-id = ID
misc-terminal-detail-name = Name
misc-terminal-detail-image = Image
misc-terminal-detail-status = Status
misc-terminal-help = Gosh Distrobox Manager shows the command to enter containers and launches your terminal. Full terminal emulation is not included.

# Images page
misc-images-title = Available Images
misc-images-subtitle = Compatible Linux distributions for new Distrobox containers (catalogue, not local images).
misc-images-search-placeholder = Search images...
misc-images-loading = Loading images…
misc-images-could-not-load = Could not load images
misc-images-empty = No images available
misc-images-empty-match = No images match your search
misc-images-custom-heading = CUSTOM IMAGE URL
misc-images-custom-placeholder = e.g. docker.io/library/ubuntu:22.04
misc-images-custom-submit = Use in Create Container
misc-images-custom-help = Enter a custom image URL to use when creating a new container.
misc-images-tag = Tag: { $tag }
misc-images-create-container = Create Container

# Updates page. The source counted with a `Container{}` suffix, which is the
# "(s)" pattern this catalogue replaces with a selector; `[one]` is therefore
# spelled out rather than left as "1 Container Available" (see
# `backup-n-snapshots` for the same decision).
misc-updates-state-upgrading = UPGRADING…
misc-updates-state-ready = READY TO UPGRADE
misc-updates-state-stopped = STOPPED
misc-updates-upgrade-action = Upgrade
misc-updates-stopped-warning = Start the container to enable upgrades.
misc-updates-empty-body = Create a container to manage updates.
misc-updates-summary = { $running } running, { $stopped } stopped. Upgrade running containers to update their packages.
misc-updates-n-containers = { $count ->
    [one] One Container Available
   *[other] { $count } Containers Available
  }
misc-updates-running-heading = RUNNING CONTAINERS
misc-updates-stopped-heading = STOPPED CONTAINERS
misc-updates-tasks-heading = UPGRADE TASKS

# ---------------------------------------------------------------------------
# Unit "views": the Dashboard, the container Details page and the shared
# shell pieces in app/src/views.rs (empty states, the three global gates, the
# container row, the clone dialog, the shared confirm modal).
#
# `dash-` is this unit's screen prefix even for copy the Details page renders,
# because both pages live in this one file.
#
# Reused shared ids (deliberately NOT duplicated here): `nav-*` for the nav
# bar titles, `action-refresh` / `action-cancel` / `action-stop` for the
# buttons, `nav-apps` for the Details "Applications" tile, and the `app-*`
# chrome — `app-quick-actions` (the QUICK ACTIONS heading on both pages),
# `app-new-container`, `app-upgrade-all`, `app-stop-all`.
#
# User data stays in placeables: `$count` on every count, `$source` / `$id`
# on the clone dialog and the Details ID line.
# ---------------------------------------------------------------------------

# Page headings (the caption_heading style renders these ALL CAPS itself)
dash-system-status = SYSTEM STATUS
dash-active-tasks = ACTIVE TASKS
dash-containers-heading = CONTAINERS
dash-container-status = CONTAINER STATUS
dash-danger-zone = DANGER ZONE

# Dashboard status card
dash-all-systems-operational = All Systems Operational
dash-attention-required = Attention Required
# `{running}`/`{total}` are numbers, and the noun count follows `total` — the
# source's `if total != 1 { "s" }` singular rule, expressed as a selector.
dash-status-running = { $total ->
    [one] { $running } of { $total } container running.
   *[other] { $running } of { $total } containers running.
  }
dash-no-containers-configured = No containers configured. Create one to get started!

# Dashboard stat tiles
dash-stat-total = Total Containers
dash-stat-running = Running
dash-stat-stopped = Stopped

# Dashboard container preview (first 5 + "View all")
dash-view-all = View all
dash-preview-empty-title = No containers yet
dash-preview-empty-body = Create your first container to get started
dash-preview-unreadable-title = Container list unreadable

# The "our list is empty, but rows were returned" copy (B3). Shared by the
# Dashboard status card / preview and by `container_list_copy` on the three
# container-gated pages, so the two surfaces cannot drift apart.
dash-no-containers = No containers found.
dash-rows-unreadable-title = { $count ->
    [one] { $count } row could not be read.
   *[other] { $count } rows could not be read.
  }
dash-rows-unreadable-body = { $count ->
    [one] distrobox reported { $count } container, but it did not match the expected format. This is usually a distrobox version mismatch.
   *[other] distrobox reported { $count } containers, but none of them matched the expected format. This is usually a distrobox version mismatch.
  }
# One-line caption (em dash, not a sentence) — the wording here is deliberately
# different from `dash-rows-unreadable-body` above, and both are kept.
dash-all-rows-failed = { $count ->
    [one] { $count } container row could not be read — it did not match the expected format.
   *[other] { $count } container rows could not be read — none of them matched the expected format.
  }
# Dashboard-only caption, gated by the `show-skipped-rows` preference.
dash-skipped-rows = { $count ->
    [one] { $count } row skipped — could not be parsed.
   *[other] { $count } rows skipped — could not be parsed.
  }

# The three global gates (§3.4)
dash-env-blocked-title = Environment Blocked
dash-distrobox-missing-title = Distrobox Not Found
dash-distrobox-missing-body = Distrobox is required to manage Linux containers. Please install it to use Gosh Distrobox Manager.
dash-check-again = Check Again

# Details page
dash-container-id = ID: { $id }
dash-upgrading = Upgrading…
dash-upgrade-container = Upgrade Container
dash-clone-container = Clone Container
dash-open-terminal = Open Terminal
dash-delete-container = Delete Container

# Clone dialog (title, body, name placeholder, confirm button)
dash-clone-dialog-title = Clone { $source }
dash-clone-dialog-body = Clone { $source } to a new container:
dash-clone-dialog-placeholder = New container name
dash-clone = Clone
