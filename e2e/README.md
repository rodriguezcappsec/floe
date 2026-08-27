# Floe native E2E

This is an opt-in Dogtail/AT-SPI suite for the actual GTK executable. It is not
part of headless `cargo test`, is not browser automation, and must run only in a
disposable graphical test session. See `docs/DEVELOPMENT.md` for dependencies,
commands, isolation guarantees, and compositor limitations.

Every application launch gets a temporary HOME, XDG config/cache/data/state and
runtime directory, plus a private freedesktop Trash. The harness refuses to use
the real HOME and never targets user mounts or normal user folders.
