# Third-party notices

OneCrew includes Teable source packages pinned to commit
`5ef2238883cad7c3980084de9a9031135fb9734f` under `vendor/teable/packages`.
These packages declare MIT licensing. `vendor/teable/LICENSE` retains the
upstream licensing statement; package license files are retained where provided.
See `vendor/teable/README.md` for the package scope and local integration changes.
No Teable `apps/` or UI source is included.

PGlite connections use `kysely-pglite-dialect` 1.3.1 (MIT), which serializes
connection acquisition for the single-session database. Its license is included
in binary assets.

Table write operations use `p-limit` (MIT) to limit concurrency through Teable's
post-commit finalization. Its license and the `yocto-queue` dependency license
are included in binary assets.

Excel import uses Teable's `@teable/v2-import` adapter. XLSX file generation uses
SheetJS Community Edition 0.20.3, the same version required by that upstream
adapter, licensed under Apache-2.0. Its license is included in binary assets.

The browser interface also installs the publicly available ReUI Data Grid
registry source through the shadcn registry (`@reui/data-grid`) and public
shadcn primitives. The registry configuration is retained in `web/components.json`.

The standalone `browser-extension/` adapts YouTube search, video, channel and
bootstrap parsing logic from OpenCLI (`clis/youtube/search.js`,
`clis/youtube/video.js`, `clis/youtube/channel.js` and
`clis/youtube/utils.js`), local
commit `8271afc67e8504bda94c147f446ee29775d08274`, licensed under Apache-2.0.
OneCrew modifications include the MV3 bridge, owned-tab lifecycle,
result schemas, pagination checks and UI. The license is retained in
`browser-extension/licenses/APACHE-2.0.txt`. OneCrew brand image assets are
from the supplied local OneCrew site.
