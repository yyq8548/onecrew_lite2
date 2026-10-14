# 0018: OpenCLI discovery Campaign storage

Status: implementing, 2026-09-24. Product authority: `../../../docs/design/onecrew-opencli-creator-discovery-v1.md` in the parent repository.

Release/storage classification for this in-progress edge: repository `HEAD` still records application 0.6.2 and Creator schema v37; there is no `v0.6.3` tag. The v38 edge exists only in this uncommitted implementation, and its exercised databases are disposable test fixtures, not a launched user store. Therefore its proposed DDL can still be completed before release. Once 0.6.3 ships, any further persisted-shape change must append v39 with a new application release.

The Creator runtime owns the durable Campaign, candidate evidence, per-platform checkpoint, and pending decision. These are not Creator library entries. OpenCLI and Chrome state are not stored here. Platform reads remain behind the local platform runtime's typed, read-only adapter. Existing one-shot search plans remain unchanged.

Schema v37 to v38 is additive: six new tables and indexes, no source-row deletion, conversion, or rewrite. A fresh database creates v38 directly. An existing database is upgraded only by the local service's established inspect, backup, validate, and single-transaction adjacent-chain upgrade; normal opening never migrates. A preflight should report zero additional destructive impact. Existing v27–v37 paths must still validate and reach v38. App release 0.6.3 is the first version with v38.

`discovery_campaigns` holds the current version and budget; `discovery_campaign_revisions` freezes every approved criteria, OpenCLI version, and scope JSON. `discovery_platform_runs` stores one scope's bounded progress and a typed stop state. `discovery_candidates` stores source-backed verdicts without creating a Creator. `discovery_pending_decisions` survives restarts. Any later schema growth requires a new edge, not modification of a released edge.

`discovery_search_hits` stores bounded URL/title/author hints and the approved keyword, not a raw result page. Finishing a search and storing its hints must commit in one transaction so a restart does not silently lose a completed search's leads.

The Campaign-wide `used_calls` counter charges every OpenCLI platform read, including video details. A platform run's `calls_used` counts searches only, so frozen keywords cannot be skipped by evidence reads. A video read reserves budget and records `video:<search-hit-id>:<call-number>` in the run's active read slot before the provider call. Completion checks the exact token and stores the candidate with its verdict in the same transaction as the checkpoint. Startup marks abandoned video reads `interrupted_video_read`; a retry charges another call and stale completion cannot overwrite it. These semantics complete the still-unreleased v38 implementation without changing its proposed DDL. Search hits and candidates survive a process restart; a failed read does not become a free call.
