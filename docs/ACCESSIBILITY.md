# Floe Accessibility

## Implemented foundations

- Keyboard routes cover navigation, views, selection, search, Settings, menus,
  Properties, Preview, Inspector, and file operations.
- Major GTK controls use semantic names, roles, descriptions, and states.
- Supported navigation and Escape dismissal return focus to the active view.
- Pane, group, warning, and operation states do not rely on color alone.
- Background activity rows distinguish running, stopping, completed, partial,
  cancelled, and failed outcomes in their accessible descriptions.
- High-contrast component contracts, 75–200% text scale, and reduced motion are
  implemented.
- Collapsible group controls are focusable and expose expanded state.

Open `Ctrl+?` for the authoritative current bindings. Optional Vim navigation
is off by default and confined to file views.

## Known limits

A complete Orca and live-announcement audit has not run. Dogtail and pyatspi are
unavailable on the current host, so native AT-SPI workflows are skipped, not
passed. Do not describe Floe as fully screen-reader verified.

Logical scaling is tested, but a physical multi-monitor fractional-scale matrix
is not complete. Wayland owns position, monitor, workspace, maximized, and
fullscreen placement.

Miller navigation follows logical RTL direction and implemented feedback paths
use first-strong isolation. Floe is otherwise English-only, has no translation
catalogs, and has no verified translated RTL walkthrough. See
[Localization](./LOCALIZATION.md).

Reports should include Floe, desktop, GTK/libadwaita, assistive-technology, and
input versions plus expected and actual focus/action. Avoid personal paths and
review [Debugging](./DEBUGGING.md) before sharing logs.
