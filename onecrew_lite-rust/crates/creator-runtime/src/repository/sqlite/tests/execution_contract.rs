use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use syn::{
    Attribute, Expr, ExprCall, ExprMethodCall, ImplItem, Item, Local, Meta, Pat, TraitItem,
    TypePath,
    punctuated::Punctuated,
    token::Comma,
    visit::{self, Visit},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ExecutionCategory {
    PureRead,
    CompositeSnapshotRead,
    SegmentedRead,
    Write,
    SegmentedWrite,
    ClaimReservation,
    MigrationMaintenance,
}

impl ExecutionCategory {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pure_read" => Some(Self::PureRead),
            "composite_snapshot_read" => Some(Self::CompositeSnapshotRead),
            "segmented_read" => Some(Self::SegmentedRead),
            "write" => Some(Self::Write),
            "segmented_write" => Some(Self::SegmentedWrite),
            "claim_reservation" => Some(Self::ClaimReservation),
            "migration_maintenance" => Some(Self::MigrationMaintenance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Free,
    Type(String),
    Trait { name: String, self_type: String },
}

impl Owner {
    fn self_type(&self) -> Option<&str> {
        match self {
            Self::Free => None,
            Self::Type(name) => Some(name),
            Self::Trait { self_type, .. } => Some(self_type),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NodeId {
    module: String,
    owner: Owner,
    function: String,
}

impl NodeId {
    fn display(&self) -> String {
        let owner = match &self.owner {
            Owner::Free => "free".to_owned(),
            Owner::Type(name) => format!("type {name}"),
            Owner::Trait { name, self_type } => format!("{name} for {self_type}"),
        };
        format!("{}::{owner}::{}", self.module, self.function)
    }
}

#[derive(Debug, Clone)]
enum CallSite {
    Function(Vec<String>),
    SelfMethod(String),
}

#[derive(Debug, Default, Clone)]
struct FunctionFacts {
    writer_locks: usize,
    read_leases: usize,
    transactions: usize,
    deferred_transactions: usize,
    deferred_on_read_lease: usize,
    calls: Vec<CallSite>,
    read_lease_bindings: BTreeSet<String>,
    closure_scopes: Vec<BTreeMap<String, bool>>,
}

fn contains_read_connection(expression: &Expr) -> bool {
    struct ReadLease(bool);
    impl<'ast> Visit<'ast> for ReadLease {
        fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
            self.0 |= expression.method == "read_connection";
            visit::visit_expr_method_call(self, expression);
        }
    }
    let mut lease = ReadLease(false);
    lease.visit_expr(expression);
    lease.0
}

fn path_expression_ident(expression: &Expr) -> Option<String> {
    let Expr::Path(path) = expression else {
        return None;
    };
    (path.path.segments.len() == 1).then(|| path.path.segments[0].ident.to_string())
}

fn is_deferred(expression: &ExprMethodCall) -> bool {
    expression.args.iter().any(|argument| {
        matches!(argument, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "Deferred"))
    })
}

fn is_closure(expression: &Expr) -> bool {
    match expression {
        Expr::Closure(_) => true,
        Expr::Paren(expression) => is_closure(&expression.expr),
        Expr::Group(expression) => is_closure(&expression.expr),
        _ => false,
    }
}

impl<'ast> Visit<'ast> for FunctionFacts {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.closure_scopes.push(BTreeMap::new());
        visit::visit_block(self, block);
        self.closure_scopes.pop();
    }

    fn visit_local(&mut self, local: &'ast Local) {
        if let Pat::Ident(binding) = &local.pat
            && let Some(initializer) = &local.init
            && contains_read_connection(&initializer.expr)
        {
            self.read_lease_bindings.insert(binding.ident.to_string());
        }
        visit::visit_local(self, local);
        if let Pat::Ident(binding) = &local.pat {
            // The initializer sees the previous binding, including inside a closure body.
            // Record other bindings too so they can shadow an outer closure binding.
            self.closure_scopes
                .last_mut()
                .expect("local declarations belong to a block")
                .insert(
                    binding.ident.to_string(),
                    local
                        .init
                        .as_ref()
                        .is_some_and(|init| is_closure(&init.expr)),
                );
        }
    }

    fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
        let method = expression.method.to_string();
        match method.as_str() {
            "lock" => self.writer_locks += 1,
            "read_connection" => self.read_leases += 1,
            "transaction" => self.transactions += 1,
            "transaction_with_behavior" => {
                self.transactions += 1;
                if is_deferred(expression) {
                    self.deferred_transactions += 1;
                    if path_expression_ident(&expression.receiver)
                        .is_some_and(|binding| self.read_lease_bindings.contains(&binding))
                    {
                        self.deferred_on_read_lease += 1;
                    }
                }
            }
            _ => {
                if path_expression_ident(&expression.receiver).as_deref() == Some("self") {
                    self.calls.push(CallSite::SelfMethod(method));
                }
            }
        }
        visit::visit_expr_method_call(self, expression);
    }

    fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
        if let Expr::Path(path) = expression.func.as_ref() {
            let local_closure = path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1
                && self
                    .closure_scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(&path.path.segments[0].ident.to_string()))
                    .copied()
                    == Some(true);
            if !local_closure {
                self.calls.push(CallSite::Function(
                    path.path
                        .segments
                        .iter()
                        .map(|segment| segment.ident.to_string())
                        .collect(),
                ));
            }
        }
        visit::visit_expr_call(self, expression);
    }
}

#[derive(Default)]
struct SqliteTypeReferences(BTreeSet<String>);

impl<'ast> Visit<'ast> for SqliteTypeReferences {
    fn visit_type_path(&mut self, path: &'ast TypePath) {
        if let Some(segment) = path.path.segments.last()
            && matches!(
                segment.ident.to_string().as_str(),
                "Connection" | "Transaction" | "Statement" | "Rows"
            )
        {
            self.0.insert(segment.ident.to_string());
        }
        visit::visit_type_path(self, path);
    }
}

struct SourceUnit {
    module: String,
    syntax: syn::File,
}

#[derive(Default)]
struct CallGraph {
    nodes: BTreeMap<NodeId, FunctionFacts>,
    names: BTreeMap<String, BTreeSet<NodeId>>,
    imports: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

impl CallGraph {
    fn insert(&mut self, node: NodeId, facts: FunctionFacts) {
        assert!(
            self.nodes.insert(node.clone(), facts).is_none(),
            "duplicate SQLite call-graph node {}",
            node.display()
        );
        self.names
            .entry(node.function.clone())
            .or_default()
            .insert(node);
    }

    fn candidates_named(&self, name: &str) -> BTreeSet<NodeId> {
        self.names.get(name).cloned().unwrap_or_default()
    }

    fn resolve(&self, caller: &NodeId, call: &CallSite) -> Result<Option<NodeId>, String> {
        match call {
            CallSite::SelfMethod(function) => self.resolve_self_method(caller, function),
            CallSite::Function(path) => self.resolve_function(caller, path),
        }
    }

    fn resolve_self_method(
        &self,
        caller: &NodeId,
        function: &str,
    ) -> Result<Option<NodeId>, String> {
        let Some(self_type) = caller.owner.self_type() else {
            return Err(format!(
                "{} calls self.{function}() without a typed owner",
                caller.display()
            ));
        };
        let candidates = self
            .candidates_named(function)
            .into_iter()
            .filter(|candidate| candidate.owner.self_type() == Some(self_type))
            .collect::<BTreeSet<_>>();
        unique_local_candidate(caller, &format!("self.{function}()"), candidates)
    }

    fn resolve_function(&self, caller: &NodeId, path: &[String]) -> Result<Option<NodeId>, String> {
        if let [owner_name, function] = path {
            let typed = self
                .candidates_named(function)
                .into_iter()
                .filter(|candidate| {
                    candidate.owner.self_type() == Some(owner_name.as_str())
                        && matches!(candidate.owner, Owner::Type(_))
                })
                .collect::<BTreeSet<_>>();
            if !typed.is_empty() {
                return unique_local_candidate(
                    caller,
                    &format!("{owner_name}::{function}()"),
                    typed,
                );
            }
        }
        let expanded;
        let path = if let Some(import) = path.first().and_then(|name| {
            self.imports
                .get(&caller.module)
                .and_then(|imports| imports.get(name))
        }) {
            expanded = import
                .iter()
                .cloned()
                .chain(path.iter().skip(1).cloned())
                .collect::<Vec<_>>();
            expanded.as_slice()
        } else {
            path
        };
        let Some(function) = path.last() else {
            return Ok(None);
        };
        let named = self.candidates_named(function);
        if named.is_empty() {
            return Ok(None);
        }

        if path.len() == 1 {
            let same_module = named
                .iter()
                .filter(|candidate| {
                    candidate.module == caller.module && candidate.owner == Owner::Free
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            if !same_module.is_empty() {
                return unique_local_candidate(caller, &format!("{function}()"), same_module);
            }
            let free = named
                .into_iter()
                .filter(|candidate| candidate.owner == Owner::Free)
                .collect::<BTreeSet<_>>();
            // A same-named method (for example Drop::drop) does not make an
            // unqualified prelude/external function a local free-function call.
            if free.is_empty() {
                return Ok(None);
            }
            return unique_local_candidate(caller, &format!("{function}()"), free);
        }

        let qualifier = &path[..path.len() - 1];
        if qualifier == ["Self"] {
            return self.resolve_self_method(caller, function);
        }
        if qualifier.len() == 1 {
            let owner_name = &qualifier[0];
            let typed = named
                .iter()
                .filter(|candidate| {
                    candidate.owner.self_type() == Some(owner_name.as_str())
                        && matches!(candidate.owner, Owner::Type(_))
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            if !typed.is_empty() {
                return unique_local_candidate(
                    caller,
                    &format!("{}::{function}()", qualifier.join("::")),
                    typed,
                );
            }
            if owner_name.chars().next().is_some_and(char::is_uppercase) {
                return Ok(None);
            }
            if matches!(
                owner_name.as_str(),
                "bool"
                    | "char"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "str"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
            ) {
                return Ok(None);
            }
        }
        if qualifier
            .last()
            .and_then(|owner| owner.chars().next())
            .is_some_and(char::is_uppercase)
        {
            return Ok(None);
        }

        let target_module = resolved_module(&caller.module, qualifier);
        let modules = named
            .into_iter()
            .filter(|candidate| {
                candidate.owner == Owner::Free
                    && (candidate.module == target_module
                        || (!qualifier.first().is_some_and(|segment| {
                            matches!(segment.as_str(), "crate" | "self" | "super")
                        }) && candidate.module.ends_with(&qualifier.join("::"))))
            })
            .collect::<BTreeSet<_>>();
        if modules.is_empty()
            && let Some(reexport) = self
                .imports
                .get(&target_module)
                .and_then(|imports| imports.get(function))
        {
            let proxy = NodeId {
                module: target_module,
                owner: Owner::Free,
                function: "<module>".to_owned(),
            };
            return self.resolve_function(&proxy, reexport);
        }
        unique_local_candidate(caller, &format!("{}()", path.join("::")), modules)
    }
}

fn unique_local_candidate(
    caller: &NodeId,
    call: &str,
    candidates: BTreeSet<NodeId>,
) -> Result<Option<NodeId>, String> {
    match candidates.len() {
        0 => Err(format!(
            "{} has unresolved local call {call}",
            caller.display()
        )),
        1 => Ok(candidates.into_iter().next()),
        _ => Err(format!(
            "{} has ambiguous local call {call}: {}",
            caller.display(),
            candidates
                .iter()
                .map(NodeId::display)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn resolved_module(caller: &str, qualifier: &[String]) -> String {
    let mut module = caller.split("::").map(str::to_owned).collect::<Vec<_>>();
    let mut index = 0;
    match qualifier.first().map(String::as_str) {
        Some("crate") => {
            module.clear();
            index = 1;
        }
        Some("self") => index = 1,
        Some("super") => {
            while qualifier.get(index).map(String::as_str) == Some("super") {
                module.pop();
                index += 1;
            }
        }
        _ => {
            module.pop();
        }
    }
    module.extend(qualifier[index..].iter().cloned());
    module.join("::")
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path_key(&path.path),
        syn::Type::Reference(reference) => format!("&{}", type_name(&reference.elem)),
        syn::Type::Slice(slice) => format!("[{}]", type_name(&slice.elem)),
        syn::Type::Tuple(tuple) => format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(type_name)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => "<unknown>".to_owned(),
    }
}

fn path_key(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| {
            let arguments = match &segment.arguments {
                syn::PathArguments::None => String::new(),
                syn::PathArguments::AngleBracketed(arguments) => format!(
                    "<{}>",
                    arguments
                        .args
                        .iter()
                        .map(|argument| match argument {
                            syn::GenericArgument::Type(ty) => type_name(ty),
                            syn::GenericArgument::Lifetime(lifetime) => lifetime.to_string(),
                            _ => "<arg>".to_owned(),
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                syn::PathArguments::Parenthesized(_) => "(<args>)".to_owned(),
            };
            format!("{}{arguments}", segment.ident)
        })
        .collect::<Vec<_>>()
        .join("::")
}

fn facts(block: &syn::Block) -> FunctionFacts {
    let mut facts = FunctionFacts::default();
    facts.visit_block(block);
    facts
}

fn call_graph(units: &[SourceUnit]) -> CallGraph {
    let mut graph = CallGraph::default();
    for unit in units {
        for item in &unit.syntax.items {
            if let Item::Use(item_use) = item {
                let mut imports = Vec::new();
                collect_use_paths(Vec::new(), &item_use.tree, &mut imports);
                let module_imports = graph.imports.entry(unit.module.clone()).or_default();
                for (alias, path) in imports {
                    if let Some(previous) = module_imports.insert(alias.clone(), path.clone()) {
                        assert_eq!(
                            previous, path,
                            "module {} imports {alias} from two paths",
                            unit.module
                        );
                    }
                }
            }
        }
        for item in &unit.syntax.items {
            match item {
                Item::Fn(function) => graph.insert(
                    NodeId {
                        module: unit.module.clone(),
                        owner: Owner::Free,
                        function: function.sig.ident.to_string(),
                    },
                    facts(&function.block),
                ),
                Item::Impl(item_impl) => {
                    let self_type = type_name(&item_impl.self_ty);
                    let owner = match &item_impl.trait_ {
                        Some((_, path, _)) => Owner::Trait {
                            name: path_key(path),
                            self_type,
                        },
                        None => Owner::Type(self_type),
                    };
                    for item in &item_impl.items {
                        let ImplItem::Fn(method) = item else {
                            continue;
                        };
                        graph.insert(
                            NodeId {
                                module: unit.module.clone(),
                                owner: owner.clone(),
                                function: method.sig.ident.to_string(),
                            },
                            facts(&method.block),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    graph
}

fn collect_use_paths(
    mut prefix: Vec<String>,
    tree: &syn::UseTree,
    output: &mut Vec<(String, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_paths(prefix, &path.tree, output);
        }
        syn::UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            output.push((name.ident.to_string(), prefix));
        }
        syn::UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            output.push((rename.rename.to_string(), prefix));
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                collect_use_paths(prefix.clone(), tree, output);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

fn reachable_facts(root: &NodeId, graph: &CallGraph) -> Result<FunctionFacts, String> {
    let mut reachable = FunctionFacts::default();
    let mut pending = vec![root.clone()];
    let mut visited = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        let direct = graph
            .nodes
            .get(&node)
            .ok_or_else(|| format!("missing call-graph node {}", node.display()))?;
        reachable.writer_locks += direct.writer_locks;
        reachable.read_leases += direct.read_leases;
        reachable.transactions += direct.transactions;
        reachable.deferred_transactions += direct.deferred_transactions;
        reachable.deferred_on_read_lease += direct.deferred_on_read_lease;
        for call in &direct.calls {
            if let Some(callee) = graph.resolve(&node, call)? {
                pending.push(callee);
            }
        }
    }
    Ok(reachable)
}

fn validate_execution(
    root: &NodeId,
    category: ExecutionCategory,
    graph: &CallGraph,
) -> Result<(), String> {
    let reachable = reachable_facts(root, graph)?;
    match category {
        ExecutionCategory::PureRead => {
            if reachable.writer_locks != 0
                || reachable.read_leases != 1
                || reachable.transactions != 0
            {
                return Err(format!(
                    "{} pure-read topology is writer={}, reader={}, transactions={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases,
                    reachable.transactions
                ));
            }
        }
        ExecutionCategory::CompositeSnapshotRead => {
            if reachable.writer_locks != 0
                || reachable.read_leases != 1
                || reachable.transactions != 1
                || reachable.deferred_transactions != 1
                || reachable.deferred_on_read_lease != 1
            {
                return Err(format!(
                    "{} composite topology is writer={}, reader={}, transactions={}, deferred={}, paired={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases,
                    reachable.transactions,
                    reachable.deferred_transactions,
                    reachable.deferred_on_read_lease
                ));
            }
        }
        ExecutionCategory::SegmentedRead => {
            // Advisory planners may compose readers; none may acquire a writer
            // or open a transaction outside a deferred read-lease snapshot.
            if reachable.writer_locks != 0
                || reachable.read_leases == 0
                || reachable.transactions != reachable.deferred_transactions
                || reachable.transactions != reachable.deferred_on_read_lease
                || reachable.transactions > reachable.read_leases
            {
                return Err(format!(
                    "{} segmented-read topology is writer={}, reader={}, transactions={}, deferred={}, paired={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases,
                    reachable.transactions,
                    reachable.deferred_transactions,
                    reachable.deferred_on_read_lease
                ));
            }
        }
        ExecutionCategory::Write => {
            if reachable.writer_locks == 0 || reachable.read_leases != 0 {
                return Err(format!(
                    "{} write topology is writer={}, reader={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases
                ));
            }
        }
        ExecutionCategory::SegmentedWrite => {
            if reachable.writer_locks != 1
                || reachable.read_leases == 0
                || reachable.transactions != reachable.read_leases + reachable.writer_locks
                || reachable.deferred_transactions != reachable.read_leases
                || reachable.deferred_on_read_lease != reachable.read_leases
            {
                return Err(format!(
                    "{} segmented-write topology is writer={}, reader={}, transactions={}, deferred={}, paired={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases,
                    reachable.transactions,
                    reachable.deferred_transactions,
                    reachable.deferred_on_read_lease
                ));
            }
        }
        ExecutionCategory::ClaimReservation => {
            if reachable.writer_locks == 0 || reachable.read_leases != 0 {
                return Err(format!(
                    "{} claim/reservation topology is writer={}, reader={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases
                ));
            }
        }
        ExecutionCategory::MigrationMaintenance => {
            if reachable.writer_locks == 0 || reachable.read_leases != 0 {
                return Err(format!(
                    "{} migration/maintenance topology is writer={}, reader={}",
                    root.display(),
                    reachable.writer_locks,
                    reachable.read_leases
                ));
            }
        }
    }
    Ok(())
}

fn crate_source(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative)
}

fn rust_files(root: &Path, skip_tests: bool) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("read repository source directory") {
            let path = entry.expect("read repository source entry").path();
            if path.is_dir() {
                if !(skip_tests && path.file_name().is_some_and(|name| name == "tests")) {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn module_name(path: &Path) -> String {
    let relative = path
        .strip_prefix(crate_source(""))
        .expect("repository source is under crate src");
    let mut segments = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let file = segments.pop().expect("Rust source has a filename");
    let stem = file.strip_suffix(".rs").expect("Rust source extension");
    if stem != "mod" {
        segments.push(stem.to_owned());
    }
    segments.join("::")
}

fn source_units(root: &Path, skip_tests: bool) -> Vec<SourceUnit> {
    rust_files(root, skip_tests)
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).expect("read Rust source");
            SourceUnit {
                module: module_name(&path),
                syntax: syn::parse_file(&source).expect("parse Rust source"),
            }
        })
        .collect()
}

fn marker(attributes: &[Attribute], owner: &str) -> ExecutionCategory {
    let values = attributes
        .iter()
        .filter_map(|attribute| {
            if !attribute.path().is_ident("cfg_attr") {
                return None;
            }
            let arguments = attribute
                .parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                .ok()?;
            if arguments.len() != 2 {
                return None;
            }
            let Meta::List(condition) = &arguments[0] else {
                return None;
            };
            if !condition.path.is_ident("any") || !condition.tokens.is_empty() {
                return None;
            }
            let Meta::List(marker) = &arguments[1] else {
                return None;
            };
            if !marker.path.is_ident("repository_execution") {
                return None;
            }
            marker
                .parse_args::<syn::Ident>()
                .ok()
                .map(|ident| ident.to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values.len(),
        1,
        "{owner} must declare exactly one cfg_attr(any(), repository_execution(category)) marker"
    );
    ExecutionCategory::parse(&values[0]).unwrap_or_else(|| {
        panic!(
            "{owner} has unknown repository execution category {}",
            values[0]
        )
    })
}

fn classified_ports() -> BTreeMap<(String, String), ExecutionCategory> {
    let mut methods = BTreeMap::new();
    for unit in source_units(&crate_source("repository/ports"), false) {
        for item in unit.syntax.items {
            let Item::Trait(repository_trait) = item else {
                continue;
            };
            let trait_name = repository_trait.ident.to_string();
            if !trait_name.ends_with("Repository") || trait_name == "CreatorRuntimeRepository" {
                continue;
            }
            for item in repository_trait.items {
                let TraitItem::Fn(method) = item else {
                    continue;
                };
                let method_name = method.sig.ident.to_string();
                let owner = format!("{trait_name}::{method_name}");
                let mut sqlite_types = SqliteTypeReferences::default();
                sqlite_types.visit_signature(&method.sig);
                assert!(
                    sqlite_types.0.is_empty(),
                    "{owner} exposes SQLite lease state in its signature: {:?}",
                    sqlite_types.0
                );
                let previous = methods.insert(
                    (trait_name.clone(), method_name),
                    marker(&method.attrs, &owner),
                );
                assert!(
                    previous.is_none(),
                    "duplicate repository port method {owner}"
                );
            }
        }
    }
    assert!(!methods.is_empty(), "no repository port methods discovered");
    methods
}

fn sqlite_implementations(graph: &CallGraph) -> BTreeMap<(String, String), NodeId> {
    let mut methods = BTreeMap::new();
    for node in graph.nodes.keys() {
        let Owner::Trait { name, self_type } = &node.owner else {
            continue;
        };
        if self_type != "SqliteRepository"
            || !name.ends_with("Repository")
            || name == "CreatorRuntimeRepository"
        {
            continue;
        }
        let key = (name.clone(), node.function.clone());
        assert!(
            methods.insert(key.clone(), node.clone()).is_none(),
            "duplicate SQLite repository method {}::{}",
            key.0,
            key.1
        );
    }
    methods
}

fn synthetic_graph(sources: &[(&str, &str)]) -> CallGraph {
    let units = sources
        .iter()
        .map(|(module, source)| SourceUnit {
            module: (*module).to_owned(),
            syntax: syn::parse_file(source).expect("parse synthetic source"),
        })
        .collect::<Vec<_>>();
    call_graph(&units)
}

fn synthetic_node(graph: &CallGraph, module: &str, owner: Owner, function: &str) -> NodeId {
    let node = NodeId {
        module: module.to_owned(),
        owner,
        function: function.to_owned(),
    };
    assert!(graph.nodes.contains_key(&node), "missing synthetic node");
    node
}

#[test]
fn every_repository_port_has_one_classification_and_one_sqlite_implementation() {
    let classified = classified_ports();
    let graph = call_graph(&source_units(&crate_source("repository/sqlite"), true));
    let implemented = sqlite_implementations(&graph);
    assert_eq!(
        classified.keys().cloned().collect::<BTreeSet<_>>(),
        implemented.keys().cloned().collect::<BTreeSet<_>>(),
        "repository ports and SQLite implementations must remain one-to-one"
    );

    let mut counts = BTreeMap::new();
    for category in classified.values() {
        *counts.entry(*category).or_insert(0_usize) += 1;
    }
    eprintln!("repository execution classification: {counts:?}");
}

#[test]
fn classified_execution_paths_have_exact_connection_ownership() {
    let classified = classified_ports();
    let graph = call_graph(&source_units(&crate_source("repository/sqlite"), true));
    let implemented = sqlite_implementations(&graph);
    for (key, category) in classified {
        validate_execution(&implemented[&key], category, &graph)
            .unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn destructor_does_not_shadow_prelude_drop_but_a_local_free_drop_is_followed() {
    let graph = synthetic_graph(&[(
        "fixture",
        "struct Guard; impl Drop for Guard { fn drop(&mut self) {} } fn root() { drop(Guard); }",
    )]);
    let root = synthetic_node(&graph, "fixture", Owner::Free, "root");
    reachable_facts(&root, &graph).expect("prelude drop is not the destructor method");

    let graph = synthetic_graph(&[(
        "fixture",
        "fn drop(repo: &Repo) { let _connection = repo.lock(); } fn root(repo: &Repo) { drop(repo); }",
    )]);
    let root = synthetic_node(&graph, "fixture", Owner::Free, "root");
    assert_eq!(reachable_facts(&root, &graph).unwrap().writer_locks, 1);
}

#[test]
fn same_named_helpers_resolve_by_module_or_fail_closed_when_unqualified() {
    let graph = synthetic_graph(&[
        (
            "fixture::reader",
            "fn helper(repo: &Repo) { let _connection = repo.read_connection(); }",
        ),
        (
            "fixture::writer",
            "fn helper(repo: &Repo) { let _connection = repo.lock(); }",
        ),
        (
            "fixture::reader::entry",
            "fn root(repo: &Repo) { super::helper(repo); }",
        ),
        (
            "fixture::ambiguous",
            "fn root(repo: &Repo) { helper(repo); }",
        ),
    ]);
    let reader = synthetic_node(&graph, "fixture::reader::entry", Owner::Free, "root");
    let facts = reachable_facts(&reader, &graph).expect("qualified helper is unique");
    assert_eq!((facts.read_leases, facts.writer_locks), (1, 0));

    let ambiguous = synthetic_node(&graph, "fixture::ambiguous", Owner::Free, "root");
    assert!(
        reachable_facts(&ambiguous, &graph)
            .unwrap_err()
            .contains("ambiguous local call")
    );
}

#[test]
fn local_closure_calls_keep_body_facts_without_cross_module_resolution() {
    let graph = synthetic_graph(&[
        ("fixture::left", "fn storage(repo: &Repo) { repo.lock(); }"),
        (
            "fixture::right",
            "fn storage(repo: &Repo) { repo.read_connection(); }",
        ),
        (
            "fixture::entry",
            "fn write(repo: &Repo) { repo.lock(); }
            fn root(repo: &Repo) {
                let storage = || {
                    let mut connection = repo.read_connection();
                    connection.transaction_with_behavior(TransactionBehavior::Deferred);
                    write(repo);
                };
                storage();
            }",
        ),
    ]);
    let root = synthetic_node(&graph, "fixture::entry", Owner::Free, "root");
    let facts = reachable_facts(&root, &graph)
        .expect("a local closure must not resolve to same-named module functions");
    assert_eq!((facts.read_leases, facts.writer_locks), (1, 1));
    assert_eq!(
        (
            facts.transactions,
            facts.deferred_transactions,
            facts.deferred_on_read_lease,
        ),
        (1, 1, 1),
    );
}

#[test]
fn closure_bindings_respect_block_scope_and_initializer_visibility() {
    let graph = synthetic_graph(&[(
        "fixture::scoped",
        "fn storage(repo: &Repo) { repo.lock(); }
        fn local_only() {
            let storage = || {};
            storage();
        }
        fn before(repo: &Repo) {
            storage(repo);
            { let storage = || {}; storage(); }
        }
        fn after(repo: &Repo) {
            { let storage = || {}; storage(); }
            storage(repo);
        }
        fn initializer(repo: &Repo) {
            let storage = || storage(repo);
            storage();
        }
        fn qualified(repo: &Repo) {
            let storage = || {};
            storage();
            self::storage(repo);
        }
        fn rebound(repo: &Repo) {
            let storage = || {};
            { let storage = self::storage; storage(repo); }
            storage();
        }",
    )]);
    for (function, expected_locks) in [
        ("local_only", 0),
        ("before", 1),
        ("after", 1),
        ("initializer", 1),
        ("qualified", 1),
        ("rebound", 1),
    ] {
        let root = synthetic_node(&graph, "fixture::scoped", Owner::Free, function);
        let facts = reachable_facts(&root, &graph).expect("scoped calls have unique targets");
        assert_eq!(facts.writer_locks, expected_locks, "{function}");
    }
}

#[test]
fn self_method_edges_expose_indirect_writer_and_reader_ownership() {
    let graph = synthetic_graph(&[(
        "fixture::owners",
        "struct Repo; impl Repo {
            fn indirect_writer(&self) { self.lock(); }
            fn indirect_reader(&self) { self.read_connection(); }
            fn read_root(&self) { self.indirect_writer(); }
            fn write_root(&self) { self.lock(); self.indirect_reader(); }
        }",
    )]);
    let read = synthetic_node(
        &graph,
        "fixture::owners",
        Owner::Type("Repo".into()),
        "read_root",
    );
    assert!(validate_execution(&read, ExecutionCategory::PureRead, &graph).is_err());
    let write = synthetic_node(
        &graph,
        "fixture::owners",
        Owner::Type("Repo".into()),
        "write_root",
    );
    assert!(validate_execution(&write, ExecutionCategory::Write, &graph).is_err());
}

#[test]
fn composite_snapshot_pairs_deferred_with_its_unique_qualified_read_lease() {
    let graph = synthetic_graph(&[
        (
            "fixture::left",
            "fn snapshot(repo: &Repo) {
                let mut connection = repo.read_connection();
                connection.transaction_with_behavior(TransactionBehavior::Deferred);
            }",
        ),
        (
            "fixture::right",
            "fn snapshot(repo: &Repo) {
                let mut connection = repo.read_connection();
                connection.transaction_with_behavior(TransactionBehavior::Deferred);
                let _other = repo.read_connection();
            }",
        ),
        (
            "fixture::entry",
            "fn root(repo: &Repo) { left::snapshot(repo); }",
        ),
    ]);
    let root = synthetic_node(&graph, "fixture::entry", Owner::Free, "root");
    validate_execution(&root, ExecutionCategory::CompositeSnapshotRead, &graph)
        .expect("qualified composite must not borrow same-named facts from another module");
}

#[test]
fn segmented_read_allows_separate_readers_but_rejects_writers_and_unpaired_transactions() {
    for (body, allowed) in [
        (
            "let _a = repo.read_connection(); let _b = repo.read_connection();",
            true,
        ),
        (
            "let mut a = repo.read_connection(); a.transaction_with_behavior(TransactionBehavior::Deferred); let _b = repo.read_connection();",
            true,
        ),
        ("let _a = repo.read_connection(); repo.lock();", false),
        (
            "let mut a = repo.read_connection(); a.transaction_with_behavior(TransactionBehavior::Immediate);",
            false,
        ),
        (
            "let _a = repo.read_connection(); other.transaction_with_behavior(TransactionBehavior::Deferred);",
            false,
        ),
        ("", false),
    ] {
        let source = format!("fn root(repo: &Repo) {{ {body} }}");
        let graph = synthetic_graph(&[("fixture", &source)]);
        let root = synthetic_node(&graph, "fixture", Owner::Free, "root");
        assert_eq!(
            validate_execution(&root, ExecutionCategory::SegmentedRead, &graph).is_ok(),
            allowed,
            "{body}"
        );
        if allowed {
            assert!(
                validate_execution(&root, ExecutionCategory::CompositeSnapshotRead, &graph)
                    .is_err()
            );
        }
    }
}
