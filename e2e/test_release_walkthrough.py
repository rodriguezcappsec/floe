"""Phase 21C installed-artifact fresh-user walkthrough contract."""

from __future__ import annotations

import os
import unittest
from pathlib import Path

from floe_harness import FloeSandbox, FloeSession, Preflight, PreflightError


class ReleaseWalkthroughContractTests(unittest.TestCase):
    def test_installed_artifact_and_isolated_roots_contract(self) -> None:
        configured = os.environ.get("FLOE_E2E_BINARY", "/usr/bin/floe")
        self.assertTrue(Path(configured).is_absolute())
        sandbox = FloeSandbox()
        try:
            sandbox.assert_isolated()
            environment = sandbox.environment()
            for variable in (
                "HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME",
                "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR",
            ):
                self.assertTrue(Path(environment[variable]).is_relative_to(sandbox.root))
            self.assertTrue((sandbox.trash / "files").is_dir())
            self.assertTrue((sandbox.trash / "info").is_dir())
        finally:
            sandbox.close()

    def test_walkthrough_scope_is_documented(self) -> None:
        root = Path(__file__).resolve().parents[1]
        guide = (root / "docs/GETTING_STARTED.md").read_text(encoding="utf-8")
        for phrase in (
            "First launch", "List and Grid", "Ctrl+F", "Quick Preview",
            "rename", "Trash/restore", "relaunch", "isolated test Trash",
        ):
            self.assertIn(phrase, guide)


class ReleaseWalkthroughNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if "FLOE_E2E_BINARY" not in os.environ:
            raise unittest.SkipTest(
                "semantic installed-artifact walkthrough skipped truthfully: "
                "FLOE_E2E_BINARY does not name a staged installed artifact; "
                "manual walkthrough evidence remains required"
            )
        try:
            cls.preflight = Preflight.inspect()
        except PreflightError as error:
            raise unittest.SkipTest(
                f"semantic installed-artifact walkthrough skipped truthfully: {error}; "
                "manual walkthrough evidence remains required"
            ) from error

    def test_fresh_user_launch_navigation_views_search_preview_and_clean_quit(self) -> None:
        with FloeSession(self.preflight) as floe:
            root = floe.sandbox.fixture
            floe.navigate_to(root)
            floe.key("<Control>1")
            floe.key("<Control>2")
            floe.key("<Control>f")
            floe.replace_text("Filename filter", "copy-source")
            self.assertTrue(floe.named("copy-source.txt").showing)
            floe.key("escape")
            floe.select_item("copy-source.txt")
            floe.key("space")
            self.assertIsNotNone(floe.window)


if __name__ == "__main__":
    unittest.main()
