# Command Execution and Blocking

The independent `crates/local-service` host projects shared application owners
through authenticated typed HTTP and raw MCP/control IPC. The CLI delegates to
this owner; React never invokes MCP. Keep handlers async, and execute blocking
filesystem/repository/encoding work through the existing blocking wrapper or
an owner's asynchronous entry point. Cheap in-memory status may run inline.

`commands.rs::web_commands!` is the single declaration of Web names, argument
keys, execution classes and typed dispatch. Unknown names fail before host
admission. Both HTTP and MCP await the concrete Creator owner APIs; Web's
vault/OS segments use `run_web_blocking`. The original `run_blocking` remains
for startup/background mechanisms, outside Web admission. Stop owns its future
outside the control count it drains. Do not restore source-string scanning or
add a catalog-ID executor to enforce these boundaries.

Acquire bounded read/write/CPU admission before dispatch. Move permits and locks
into the full blocking operation: caller cancellation does not stop that worker.
Hold a Creator store lock for one bounded repository call; never across provider I/O,
loops of per-record queries, or serialization. Use owner batch queries.

Startup alone owns data preflight/backup/adjacent upgrades/validation/recovery,
after exclusive ownership and old-process checks. The StartupPhase projection
is Inspecting, Upgrading, Ready, or Failed. Business intake requires Ready;
limited status/MCP diagnostics remain available earlier. Compatible data starts
directly. Failure blocks business even if a partial commit changed schema state.
No CLI, HTTP, MCP, or browser action executes upgrades; data status is read-only.

Supervised work must retain its owning task and report terminal outcomes.
Close intake, signal cooperative cancellation, drain with a bounded deadline,
and classify timeout. A try_lock failure may skip optional work, never decide
correctness or readiness. Never nest blocking wrappers or use block_on inside
a runtime worker. Test overload, cancellation, startup failure, and drain behavior
with the responsible owner; real responsiveness needs a native runtime observation.
