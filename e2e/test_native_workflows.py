from __future__ import annotations

import unittest

from floe_harness import FloeSession, Preflight, PreflightError


class NativeWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        try:
            cls.preflight = Preflight.inspect()
        except PreflightError as error:
            raise unittest.SkipTest(str(error)) from error

    def session(self) -> FloeSession:
        return FloeSession(self.preflight)

    def test_e2e_01_launch_responsive_and_clean_quit(self) -> None:
        with self.session() as floe:
            self.assertIsNotNone(floe.window)
            floe.key("<Control>l")
            self.assertTrue(floe.named("Folder location").showing)
            floe.key("escape")

    def test_e2e_02_navigation_back_forward_parent(self) -> None:
        with self.session() as floe:
            root = floe.sandbox.fixture
            child = root / "child"
            floe.navigate_to(root)
            floe.navigate_to(child)
            floe.key("<Alt>Left")
            floe.wait_for_location(root)
            floe.key("<Alt>Right")
            floe.wait_for_location(child)
            floe.key("<Alt>Up")
            floe.wait_for_location(root)

    def test_e2e_03_create_and_rename(self) -> None:
        with self.session() as floe:
            root = floe.sandbox.fixture
            created = root / "created-folder"
            renamed = root / "renamed-folder"
            floe.navigate_to(root)
            floe.key("<Control><Shift>n")
            floe.replace_text("New item name", created.name)
            floe.activate("Create")
            floe.wait_exists(created)
            floe.select_item(created.name)
            floe.key("f2")
            floe.replace_text("New filename", renamed.name)
            floe.activate("Rename")
            floe.wait_exists(renamed)
            floe.wait_missing(created)

    def test_e2e_04_copy_and_move(self) -> None:
        with self.session() as floe:
            root = floe.sandbox.fixture
            destination = root / "destination"
            floe.navigate_to(root)
            floe.select_item("copy-source.txt")
            floe.key("<Control>c")
            floe.navigate_to(destination)
            floe.key("<Control>v")
            floe.wait_exists(destination / "copy-source.txt")

            floe.navigate_to(root)
            floe.select_item("move-source.txt")
            floe.key("<Control>x")
            floe.navigate_to(destination)
            floe.key("<Control>v")
            floe.wait_exists(destination / "move-source.txt")
            floe.wait_missing(root / "move-source.txt")

    def test_e2e_05_search_current_folder(self) -> None:
        with self.session() as floe:
            floe.navigate_to(floe.sandbox.fixture)
            floe.key("<Control>f")
            floe.replace_text("Filename filter", "needle unique")
            result = floe.named("needle unique result.txt")
            self.assertTrue(result.showing)

    def test_e2e_06_isolated_trash_and_restore(self) -> None:
        with self.session() as floe:
            original = floe.sandbox.fixture / "trash-me.txt"
            floe.navigate_to(floe.sandbox.fixture)
            floe.select_item(original.name)
            floe.key("delete")
            floe.wait_missing(original)
            floe.palette("Open Trash")
            floe.select_item(original.name)
            floe.palette("Restore")
            floe.wait_exists(original)

    def test_e2e_07_multi_selection_batch_copy(self) -> None:
        with self.session() as floe:
            root = floe.sandbox.fixture
            destination = root / "destination"
            floe.navigate_to(root)
            floe.select_item("batch-a.txt")
            floe.select_item("batch-b.txt", extend=True)
            floe.key("<Control>c")
            floe.navigate_to(destination)
            floe.key("<Control>v")
            floe.wait_exists(destination / "batch-a.txt")
            floe.wait_exists(destination / "batch-b.txt")

    def test_e2e_08_keyboard_workflow(self) -> None:
        with self.session() as floe:
            root = floe.sandbox.fixture
            child = root / "child"
            floe.navigate_to(root)
            floe.key("<Control>f")
            self.assertTrue(floe.named("Filename filter").showing)
            floe.key("escape")
            floe.key("<Control>t")
            self.assertIsNotNone(floe.named(f"Tab: {root.name}"))
            floe.key("<Control>w")
            floe.navigate_to(child)
            floe.key("<Alt>Left")
            floe.wait_for_location(root)
            floe.key("<Alt>Right")
            floe.wait_for_location(child)
            floe.key("<Alt>Up")
            floe.wait_for_location(root)
            floe.select_item("copy-source.txt")
            floe.key("f2")
            self.assertTrue(floe.named("New filename").showing)
            floe.key("escape")


if __name__ == "__main__":
    unittest.main()
