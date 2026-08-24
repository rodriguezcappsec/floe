# Gates: Storage integration

Scope: GIO snapshots and actions provide a coherent application-owned device boundary.

- [x] N1: Leaf 1.2.1 reports every gate met.
  CHECK: node <unlazy-skill-dir>/scripts/gate-check.mjs --status gates/leaf-1.2.1.md
  EXPECT: /ALL MET/
  EVIDENCE: Parent re-ran the status checker after hierarchy-row deduplication: `ALL MET (6 met)`.
