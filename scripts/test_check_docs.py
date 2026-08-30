#!/usr/bin/env python3
"""Focused regression contracts for the Phase 21C documentation checker."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


CHECKER_PATH = Path(__file__).with_name("check-docs.py")
SPEC = importlib.util.spec_from_file_location("floe_check_docs", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class DocumentationCheckerTests(unittest.TestCase):
    def test_duplicate_base_slug_is_rejected_but_numbered_anchor_is_retained(self) -> None:
        errors: list[str] = []
        anchors = CHECKER.document_anchors(
            Path("fixture.md"), ["# Repeated heading", "## Repeated heading"], errors
        )

        self.assertEqual(anchors, {"repeated-heading", "repeated-heading-1"})
        self.assertEqual(len(errors), 1)
        self.assertIn("duplicate heading slug 'repeated-heading'", errors[0])

    def test_table_contract_applies_to_any_release_document(self) -> None:
        errors: list[str] = []
        CHECKER.table_checks(
            Path("docs/GETTING_STARTED.md"),
            ["| A | B |", "| --- | --- |", "| one | two | three |"],
            errors,
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("table has 3 columns; expected 2", errors[0])


if __name__ == "__main__":
    unittest.main()
