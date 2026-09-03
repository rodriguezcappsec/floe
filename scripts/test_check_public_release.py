#!/usr/bin/env python3
"""Focused identity-policy regressions for the public-release audit."""

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-public-release.py")
SPEC = importlib.util.spec_from_file_location("check_public_release", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CommitIdentityPolicyTests(unittest.TestCase):
    def test_reviewed_noreply_identity_is_allowed_for_ordinary_commits(self) -> None:
        self.assertTrue(
            MODULE.commit_identity_is_public(
                "parent",
                MODULE.PUBLIC_COMMIT_EMAIL,
                "Luis",
                MODULE.PUBLIC_COMMIT_EMAIL,
            )
        )

    def test_exact_github_identity_is_allowed_only_for_merge_commits(self) -> None:
        arguments = (
            MODULE.PUBLIC_COMMIT_EMAIL,
            MODULE.GITHUB_MERGE_COMMITTER[0],
            MODULE.GITHUB_MERGE_COMMITTER[1],
        )
        self.assertTrue(MODULE.commit_identity_is_public("left right", *arguments))
        self.assertFalse(MODULE.commit_identity_is_public("parent", *arguments))
        self.assertFalse(
            MODULE.commit_identity_is_public(
                "left right", arguments[0], "Not GitHub", arguments[2]
            )
        )

    def test_unreviewed_author_or_committer_is_rejected(self) -> None:
        self.assertFalse(
            MODULE.commit_identity_is_public(
                "parent", "private@example.test", "Luis", MODULE.PUBLIC_COMMIT_EMAIL
            )
        )
        self.assertFalse(
            MODULE.commit_identity_is_public(
                "parent", MODULE.PUBLIC_COMMIT_EMAIL, "Luis", "private@example.test"
            )
        )


if __name__ == "__main__":
    unittest.main()
