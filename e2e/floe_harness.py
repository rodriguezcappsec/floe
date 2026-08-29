"""Dogtail/AT-SPI harness with strict temporary desktop-data isolation.

The module intentionally imports Dogtail only after preflight so ordinary
Python discovery can verify isolation contracts on machines without graphical
automation dependencies.
"""

from __future__ import annotations

import importlib.util
import os
import stat
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, TypeVar


SCENARIO_IDS = (
    "E2E-01",
    "E2E-02",
    "E2E-03",
    "E2E-04",
    "E2E-05",
    "E2E-06",
    "E2E-07",
    "E2E-08",
)

_T = TypeVar("_T")


class PreflightError(RuntimeError):
    """The native E2E environment is unavailable or unsafe."""


@dataclass(frozen=True)
class Preflight:
    binary: Path

    @classmethod
    def inspect(cls) -> "Preflight":
        missing: list[str] = []
        if importlib.util.find_spec("dogtail") is None:
            missing.append("Python dogtail")
        if importlib.util.find_spec("pyatspi") is None:
            missing.append("Python pyatspi/AT-SPI")
        if not os.environ.get("WAYLAND_DISPLAY"):
            missing.append("a Wayland display")
        if not os.environ.get("XDG_RUNTIME_DIR"):
            missing.append("XDG_RUNTIME_DIR")
        if not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
            missing.append("a D-Bus session bus")

        configured = os.environ.get("FLOE_E2E_BINARY")
        binary = (
            Path(configured).expanduser()
            if configured
            else Path(__file__).resolve().parents[1] / "target" / "debug" / "floe"
        )
        if not binary.is_file() or not os.access(binary, os.X_OK):
            missing.append(f"built Floe executable at {binary}")
        if missing:
            raise PreflightError("native E2E unavailable: " + ", ".join(missing))
        return cls(binary=binary.resolve())


class FloeSandbox:
    """Private HOME/XDG/Trash and fixtures for one native application launch."""

    def __init__(self) -> None:
        self._temporary = tempfile.TemporaryDirectory(prefix="floe-native-e2e-")
        self.root = Path(self._temporary.name).resolve()
        self.home = self.root / "home"
        self.config = self.root / "config"
        self.cache = self.root / "cache"
        self.data = self.root / "data"
        self.state = self.root / "state"
        self.runtime = self.root / "runtime"
        self.trash = self.data / "Trash"

        for directory in (
            self.home,
            self.config,
            self.cache,
            self.data,
            self.state,
            self.runtime,
            self.trash,
            self.trash / "files",
            self.trash / "info",
        ):
            directory.mkdir(parents=True, exist_ok=True, mode=0o700)
            directory.chmod(0o700)

        self._bridge_wayland_socket()
        self.fixture = self.home / "Floe E2E Fixture"
        self.fixture.mkdir(mode=0o700)
        (self.fixture / "child").mkdir()
        (self.fixture / "destination").mkdir()
        for name, contents in {
            "copy-source.txt": b"copy fixture\n",
            "move-source.txt": b"move fixture\n",
            "needle unique result.txt": b"search fixture\n",
            "trash-me.txt": b"trash fixture\n",
            "batch-a.txt": b"batch a\n",
            "batch-b.txt": b"batch b\n",
        }.items():
            (self.fixture / name).write_bytes(contents)

    def _bridge_wayland_socket(self) -> None:
        display = os.environ.get("WAYLAND_DISPLAY", "")
        source_runtime = Path(os.environ.get("XDG_RUNTIME_DIR", "/nonexistent"))
        if not display or os.path.isabs(display):
            return
        source = source_runtime / display
        if source.exists():
            (self.runtime / display).symlink_to(source)

    def environment(self) -> dict[str, str]:
        environment = os.environ.copy()
        environment.update(
            {
                "HOME": str(self.home),
                "XDG_CONFIG_HOME": str(self.config),
                "XDG_CACHE_HOME": str(self.cache),
                "XDG_DATA_HOME": str(self.data),
                "XDG_STATE_HOME": str(self.state),
                "XDG_RUNTIME_DIR": str(self.runtime),
                "GSETTINGS_BACKEND": "memory",
                "GDK_BACKEND": "wayland",
                "FLOE_SESSION_POLICY": "private",
                "RUST_LOG": "floe_app=warn,floe_core=warn",
            }
        )
        return environment

    def assert_isolated(self) -> None:
        real_home = Path.home().resolve()
        for path in (self.home, self.config, self.cache, self.data, self.state, self.trash):
            if not path.is_relative_to(self.root):
                raise AssertionError(f"E2E path escaped sandbox: {path}")
            if path == real_home:
                raise AssertionError("E2E path resolved to the real HOME")
        if stat.S_IMODE(self.runtime.stat().st_mode) != 0o700:
            raise AssertionError("isolated XDG_RUNTIME_DIR is not mode 0700")

    def close(self) -> None:
        self._temporary.cleanup()


def wait_until(
    description: str,
    probe: Callable[[], _T | None],
    *,
    timeout: float = 12.0,
) -> _T:
    """Wait for a concrete accessibility/process/filesystem condition."""

    deadline = time.monotonic() + timeout
    wake = threading.Event()
    last_error: BaseException | None = None
    while time.monotonic() < deadline:
        try:
            value = probe()
            if value:
                return value
        except (LookupError, RuntimeError) as error:
            last_error = error
        wake.wait(0.05)
    detail = f"; last error: {last_error}" if last_error else ""
    raise AssertionError(f"timed out waiting for {description}{detail}")


class FloeSession:
    """One actual Floe process controlled through Dogtail's AT-SPI surface."""

    def __init__(self, preflight: Preflight) -> None:
        self.preflight = preflight
        self.sandbox = FloeSandbox()
        self.sandbox.assert_isolated()
        self.process: subprocess.Popen[bytes] | None = None
        self.app = None
        self.window = None
        self._search_errors: tuple[type[BaseException], ...] = (LookupError,)

    def start(self) -> "FloeSession":
        from dogtail.config import config
        from dogtail.tree import SearchError, root

        config.actionDelay = 0
        config.defaultDelay = 0
        config.searchCutoffCount = 20
        self._search_errors = (LookupError, SearchError)
        self.process = subprocess.Popen(
            [str(self.preflight.binary)],
            cwd=self.sandbox.fixture,
            env=self.sandbox.environment(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        def find_application():
            if self.process is not None and self.process.poll() is not None:
                stderr = self.process.stderr.read().decode("utf-8", "replace")
                raise RuntimeError(f"Floe exited during launch: {stderr[-2000:]}")
            for name in ("floe", "Floe", "io.github.rodriguezcappsec.Floe"):
                try:
                    return root.application(name)
                except self._search_errors:
                    continue
            return None

        self.app = wait_until("Floe AT-SPI application", find_application)
        self.window = wait_until(
            "Floe main window",
            lambda: self._optional_child("Floe", role_name="frame"),
        )
        return self

    def _optional_child(self, name: str, *, role_name: str | None = None):
        if self.app is None:
            return None
        try:
            return self.app.child(
                name=name,
                roleName=role_name,
                recursive=True,
                showingOnly=True,
            )
        except self._search_errors:
            return None

    def named(self, name: str, *, role_name: str | None = None):
        return wait_until(
            f"accessible node {name!r}",
            lambda: self._optional_child(name, role_name=role_name),
        )

    def key(self, accelerator: str) -> None:
        from dogtail.rawinput import keyCombo

        keyCombo(accelerator)

    def replace_text(self, accessible_name: str, text: str) -> None:
        entry = self.named(accessible_name)
        entry.grabFocus()
        entry.text = text

    def activate(self, accessible_name: str) -> None:
        node = self.named(accessible_name)
        actions = set(node.actions)
        for action in ("click", "activate", "press"):
            if action in actions:
                node.doActionNamed(action)
                return
        node.grabFocus()
        self.key("enter")

    def select_item(self, name: str, *, extend: bool = False) -> None:
        item = self.named(name)
        item.grabFocus()
        if extend:
            self.key("<Control>space")
            return
        if "click" in set(item.actions):
            item.doActionNamed("click")
        else:
            # Ctrl+Space is GTK's native selection toggle and avoids Floe's
            # unmodified Space quick-preview action.
            self.key("<Control>space")

    def navigate_to(self, path: Path) -> None:
        self.key("<Control>l")
        self.replace_text("Folder location", str(path))
        self.key("enter")
        self.wait_for_location(path)

    def current_location(self) -> Path:
        self.key("<Control>l")
        entry = self.named("Folder location")
        location = Path(entry.text)
        self.key("escape")
        return location

    def wait_for_location(self, path: Path) -> None:
        expected = path.resolve()

        def location_matches():
            self.key("<Control>l")
            entry = self.named("Folder location")
            value = Path(entry.text)
            self.key("escape")
            return value.resolve() == expected

        wait_until(f"location {expected}", location_matches)

    def palette(self, command: str) -> None:
        self.key("<Control><Shift>p")
        self.replace_text("Search Floe commands", command)
        self.activate(command)

    def wait_exists(self, path: Path) -> None:
        wait_until(f"filesystem path {path}", lambda: path.exists())

    def wait_missing(self, path: Path) -> None:
        wait_until(f"filesystem path {path} to disappear", lambda: not path.exists())

    def close(self) -> None:
        if self.process is not None and self.process.poll() is None:
            try:
                if self.window is not None:
                    self.window.grabFocus()
                    self.key("<Control>q")
                self.process.wait(timeout=5)
            except (AssertionError, LookupError, subprocess.TimeoutExpired):
                self.process.terminate()
                try:
                    self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=3)
        self.sandbox.close()

    def __enter__(self) -> "FloeSession":
        return self.start()

    def __exit__(self, exc_type, exc, traceback) -> None:
        self.close()
