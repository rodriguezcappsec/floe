# Gates: Local navigation sources integration

Scope: Places and bookmarks compose without losing exact path identity.

- [x] N1: Leaf 1.1.1 reports every gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status gates/leaf-1.1.1.md
  EXPECT: /ALL MET/
  EVIDENCE: Parent re-ran the status checker: `ALL MET (5 met)`.
