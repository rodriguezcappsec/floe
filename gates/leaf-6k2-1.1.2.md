# Gates: Phase 6K2 privileged browsing security design

Scope: Implementable threat model and architecture for a future real Open as Administrator action without elevating the Floe UI.

- [x] L1: Design rejects whole-application root, shell interpolation, password capture, and lossy path reconstruction.
  CHECK: rg -n 'whole.*root|shell|password|lossy|PathBuf|GFile' docs/PRIVILEGED_ACCESS.md
  EXPECT: /whole.*root/
  EVIDENCE: docs/PRIVILEGED_ACCESS.md:14-19 rejects root UI, helpers, shell/path interpolation, and password handling; lines 42-58 require exact raw PathBuf plus canonical GFile URI identity and reject lossy reconstruction/authority fallback.

- [x] L2: Design specifies GFile `admin://`, GVfs/polkit capability detection, authenticated enumeration and jobs, visible privileged state, downgrade/close, and failure fallback.
  CHECK: rg -n 'admin://|GVfs|polkit|enumeration|job|badge|downgrade|fallback' docs/PRIVILEGED_ACCESS.md
  EXPECT: /admin:\/\//
  EVIDENCE: docs/PRIVILEGED_ACCESS.md:128-160 defines provider/command/job routing; lines 162-208 define nonprompting capability detection, async authentication, classified fallback, and state transitions; lines 210-245 define persistent badge plus downgrade/close behavior.

- [x] L3: Design defines test and rollout gates before the user-facing action may be enabled.
  CHECK: rg -n 'Test gates|Rollout gates|must not be exposed|non-UTF-8|symlink|timeout' docs/PRIVILEGED_ACCESS.md
  EXPECT: /must not be exposed/
  EVIDENCE: docs/PRIVILEGED_ACCESS.md:295-324 specifies nine test gates; lines 326-344 specify six staged rollout gates; lines 346-348 prohibit exposing a working action before applicable gates pass.
