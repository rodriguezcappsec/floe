from __future__ import annotations

import os
import stat
import unittest
from pathlib import Path

from floe_harness import FloeSandbox, SCENARIO_IDS
from test_native_workflows import NativeWorkflowTests


class HarnessContractTests(unittest.TestCase):
    def test_all_initial_scenarios_have_stable_ids(self) -> None:
        self.assertEqual(SCENARIO_IDS, tuple(f"E2E-{index:02d}" for index in range(1, 9)))

    def test_native_workflow_suite_registers_all_eight_scenarios(self) -> None:
        methods = unittest.TestLoader().getTestCaseNames(NativeWorkflowTests)
        self.assertEqual(len(methods), 8)
        for index, method in enumerate(methods, start=1):
            self.assertIn(f"e2e_{index:02d}", method)

    def test_home_xdg_and_trash_roots_are_private_and_temporary(self) -> None:
        sandbox = FloeSandbox()
        try:
            sandbox.assert_isolated()
            environment = sandbox.environment()
            for variable in (
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "XDG_RUNTIME_DIR",
            ):
                path = Path(environment[variable]).resolve()
                self.assertTrue(path.is_relative_to(sandbox.root), variable)
                self.assertNotEqual(path, Path(os.environ.get(variable, "/nonexistent")).resolve())
            self.assertTrue((sandbox.trash / "files").is_dir())
            self.assertTrue((sandbox.trash / "info").is_dir())
            self.assertEqual(stat.S_IMODE(sandbox.trash.stat().st_mode), 0o700)
        finally:
            sandbox.close()


if __name__ == "__main__":
    unittest.main()
