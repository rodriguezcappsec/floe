# Gates: Sidebar integration

Scope: Local sources and devices compose in one responsive accessible sidebar.

- [x] N1: Leaf 1.3.1 reports every gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status gates/leaf-1.3.1.md
  EXPECT: /ALL MET/
  EVIDENCE: Parent re-ran the status checker: `ALL MET (3 met)`.
