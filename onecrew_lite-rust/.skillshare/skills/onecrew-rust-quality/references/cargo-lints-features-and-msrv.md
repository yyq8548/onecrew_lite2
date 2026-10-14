# Cargo, Lints, Features, and MSRV

Use this pattern for Rust/Cargo lint policy, suppression, workspace inheritance,
features, resolver behavior, MSRV, or publish posture. Official sources:
[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html),
[lints](https://doc.rust-lang.org/cargo/reference/lints.html),
[`rust-version`](https://doc.rust-lang.org/cargo/reference/rust-version.html),
[features](https://doc.rust-lang.org/cargo/reference/features.html),
[resolver versions](https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions),
[manifest publish policy](https://doc.rust-lang.org/cargo/reference/manifest.html#the-publish-field),
[SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html),
and [Clippy usage](https://doc.rust-lang.org/stable/clippy/usage.html).

## Critical Rules

First observe declarations and effective semantics. Root workspace tables are
not automatically inherited: members opt into workspace package, dependency,
and lint settings through their manifest fields. Resolver and feature
unification can change the effective graph. Report explicit, inherited,
implicit, and unresolved states separately before recommending a policy.

```rust
// ❌ BAD: defaults and one build are reported as deliberate workspace policy.
let policy = PolicyReport::accepted(CargoDefaults::current());

// ✅ GOOD: the report preserves declarations, inheritance, and gaps.
let policy = PolicyReport::inspect(&workspace_metadata)?;
for gap in policy.unresolved_decisions() {
    recommendations.push(gap.scoped_to_affected_packages());
}
```

Never enable all `clippy::restriction`; its lints may conflict or prohibit
reasonable language features. `clippy::pedantic` intentionally produces false
positives and may be adopted as a whole with local exceptions or cherry-picked.
Choose lint groups and exceptions from project risk, not maximum strictness.

## Quick Reference

| Area | Inspect and report |
|---|---|
| Workspace coverage | Members, default-members, virtual root, actual command selection |
| Workspace lints | Root declaration, each member's inheritance, effective levels |
| Suppression | Lint, scope, reason, permanence, and whether `expect` can expire |
| MSRV | Explicit `rust-version`, package policy, feature/target coverage, evidence |
| Resolver | Explicit workspace value, edition-derived default, feature/version impact |
| Features | Default and implicit features, additive unification, mutually exclusive risks |
| Publish | Explicit registry list or `publish = false`; absence is not release intent |
| SemVer | Public API/feature/MSRV impact as guidance, not an automatic version action |

## Common Patterns

### Narrow suppression with a reason

```rust
// ❌ BAD: a module-wide allow hides both intentional and accidental findings.
#![allow(clippy::all, dead_code)]

// ✅ GOOD: an expected exception is local and fails when it becomes stale.
#[expect(clippy::too_many_arguments, reason = "wire constructor mirrors protocol fields")]
fn from_wire(/* protocol fields */) -> Message { /* ... */ }
```

Use `allow` instead when the exception is conditionally absent or intentionally
permanent, and still attach a reason. Do not lower a lint merely to make an
unrelated gate pass. A toolchain update can add warnings; `-D warnings` is an
explicit stability tradeoff, not a timeless best practice.

### MSRV is a support contract, not an observation

`rust-version` states the supported minimum for a package. The local compiler,
edition, resolver, lockfile, or one successful build does not establish MSRV.
Recommend a policy and affected matrix; do not choose or raise the value, alter
dependencies, or claim support without explicit authorization and evidence.

### Features are additive and lane-specific

Dependency features normally unify. One `default-features = false` edge cannot
guarantee defaults are absent if another edge enables them. A workspace
all-features build is a merged state and may be invalid for mutually exclusive
features; select default, no-default, package, and feature combinations from
the supported contract.

### Publish absence is unresolved policy

`publish = false` is an explicit safeguard. A missing `publish` field does not
prove intent to publish. Report the packages and risk, then leave manifest,
package, registry, version, and release changes to separately authorized work.

### Formatting remains delegated

The [Rust Style Guide](https://doc.rust-lang.org/style-guide/) describes default
Rust style and informs rustfmt. Use the configured rustfmt policy and report
formatting as a separate evidence lane; do not duplicate style rules or trigger
an edition-wide format migration from this practice reference.

## Checklist

- [ ] Workspace members/default-members and inheritance are observed exactly.
- [ ] Declared policy, effective semantics, implicit defaults, and gaps differ.
- [ ] Lint groups are scoped; restriction/pedantic are not blanket mandates.
- [ ] Suppressions are narrow, justified, and use `expect` or `allow` correctly.
- [ ] MSRV is explicit and separately evidenced per supported package/lane.
- [ ] Feature unification/defaults/mutual exclusion shape the proposed matrix.
- [ ] Resolver or publish absence is not mislabeled as deliberate policy.
- [ ] No manifest, dependency, lockfile, package, publish, or release mutation occurs without scope.
- [ ] Exact command inventories remain outside this practice reference.
