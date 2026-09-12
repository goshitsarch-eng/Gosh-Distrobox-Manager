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
app-about = About

# Navigation
nav-dashboard = Dashboard
nav-containers = Containers
nav-images = Images
nav-packages = Packages
nav-updates = Updates
nav-backups = Backups
nav-apps = Apps
nav-terminal = Terminal
nav-activity = Activity
nav-settings = Settings

# Generic actions
action-refresh = Refresh
action-retry = Retry
action-cancel = Cancel
action-close = Close
action-back = Back
action-copy = Copy
action-copied = Copied
action-start = Start
action-stop = Stop
action-remove = Remove
action-create = Create
action-search = Search

# States
state-loading = Loading…
state-error = Error
state-empty = Nothing here yet
