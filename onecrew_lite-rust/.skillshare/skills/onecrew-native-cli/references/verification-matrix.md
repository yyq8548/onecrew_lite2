# Native Verification Matrix

Select native evidence from the actual host, target, toolchain, effect, and
changed owner. Never encode a permanent “development machine” versus “user's
Mac” split in reusable guidance.

## Preconditions

- Record the executing host OS/architecture, requested target, installed
  toolchain/SDK, and authorization boundary.
- Identify the shared contract and exact native mechanic that changed.
- Separate compile, unit, raw-binary identity, native runtime, and live workflow claims.
- Resolve standard commands through `onecrew-verify`; this matrix owns
  behavior rows, not command spelling.

## Workflow

1. Select every affected row:

   | Change | Automated evidence | Native acceptance |
   |---|---|---|
   | Shared Rust contract | Focused owner tests, then one final touched-owner gate | None unless native mechanics changed |
   | OS API or `cfg` branch | Target compile plus pure branch tests | Behavior on a supported target host |
   | Browser discovery | Side-effect-free probe and capability projection tests | Absent/present discovery without launching ordinary Chrome |
   | Browser session | Lifecycle, timeout, cleanup, and profile isolation tests | Explicitly authorized launch/close/reconnect smoke |
   | Filesystem/security | Path, permission, and secret-file boundary tests | Supported host path and permission behavior |
   | Raw binary | Embedded asset and version/target/SHA256/size manifest checks | Actual destination-host launch and shutdown; report OS rejection |

2. Run focused affected checks during implementation.
3. At final handoff, run or reuse one complete evidence identity per touched
   owner and add only immediate shared-contract consumers.
4. Hand-audit cross-language DTO mirrors when their source shape changed.
5. Label each result with host, target, architecture, evidence level, and
   explicit authorization. Mark unavailable native rows `BLOCKED`.

## Failure recovery

- Fix a failed owner lane at its source; never remove warning denial, weaken a
  test, or widen scope to dilute the failure.
- Correct a drifted cross-language mirror explicitly; do not add a default or
  optional compatibility field to hide it.
- Treat a missing target, SDK, service, or user authorization as
  blocked coverage, not as a request to substitute a cross-build.
- If a native run would launch, install, overwrite, sign, publish, or mutate
  durable state without authority, stop and request the missing scope.

## Verification

- Every selected row has honest automated or native-host evidence, an identical
  reused `PASS`, or an explicit `BLOCKED` record.
- Cross-compilation is described only as compile/link evidence; artifact hash
  equality is described only as byte identity.
- No static, mock, readiness, or foreign-host result is presented as native,
  live, signing, installer, or release success.
- The final report states exactly what ran, where, what was reused, and what was
  not run.

## Stop condition

Stop when every affected row is evidenced or blocked at its honest level and
no unauthorized launch, packaging, signing, installation, publication, or
native-runtime claim remains. User acceptance of the delivery ends the old
verification loop.
