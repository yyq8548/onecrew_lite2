# Upstream libraries

The source trees in this directory are intentionally committed with the main
repository. They are not Git submodules.

- `chat-xdk`: `https://github.com/xdevplatform/chat-xdk`, based on `b9d8d44`
  (`v0.4.3`).
- `juicebox-sdk`: `https://github.com/juicebox-systems/juicebox-sdk`, based on
  `14fb45a75473a2cb053e5d9c17873085550ba51f`.

Import upstream changes as a dedicated commit and review the resulting source
diff before merging. Do not replace either directory during a feature change.
The prebuilt Go archives under `chat-xdk/go/chatxdk/libs/` are intentionally
excluded from the parent repository because the desktop application consumes
the Rust crate and those archives are generated release artifacts.
