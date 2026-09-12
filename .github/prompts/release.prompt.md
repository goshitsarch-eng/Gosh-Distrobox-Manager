---
description: 'Prepare the next release'
---
Your task is to prepare the next release:
1. Analyze the recent changes in the codebase, using git commit messages and diffs.
Use the following git commands to gather the necessary information:
```sh
git --no-pager log --reverse $(git --no-pager describe --tags --abbrev=0)..HEAD --pretty=format:"commit %H%nAuthor: %an <%ae>%nDate: %ad%nSubject: %s%n%n%b" --date=short -p --no-color
```
2. Identify significant features, bug fixes, and improvements.
3. Write clear and concise release notes summarizing these changes, appending them to
   `core/data/io.github.gosh_distrobox_manager.metainfo.xml`. Ensure you add the new release
   entry at the top of the `<releases>` section with the current date.
   (This file used to be `rust/data/io.github.gosh_distrobox_manager.metainfo.xml.in`, a
   template consumed by the meson build. Both the `rust/` crate path and the `.in` template
   are gone — the file is now the real, installed metainfo, and there is one copy of it.)
4. Look at the version history in that same `metainfo.xml` to determine the next version
   number, following semantic versioning principles.
5. Update the version number in `core/Cargo.toml`. That file is the single source of truth
   for the version (D14) — there is no root `meson.build` any more (it was deleted with the
   Flutter rewrite, and this step previously referenced it, which is why it silently failed).
   The version must agree across `core/Cargo.toml`, `app/Cargo.toml`, `Cargo.lock`,
   `gosh-distrobox-manager.spec`, the metainfo `<releases>` entry, `build-rpm.sh` and
   `RPM-BUILD.md`. `scripts/check-versions.sh` (T4) greps all of them and fails on
   disagreement; run it rather than checking by eye.
6. Do a final git commit with the message "vX.Y.Z". Finally, tag the commit with "vX.Y.Z".
