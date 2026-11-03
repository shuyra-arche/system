#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    use syn::{ItemStruct, Signature, visit::{self, Visit}};
    #[derive(Default)]
    struct Signatures { violations: Vec<String>, count: usize }
    impl Signatures {
        fn check(&mut self, signature: &Signature) {
            self.count += 1;
            if signature.inputs.len() > 3 {
                self.violations.push(format!("{} has {} parameters", signature.ident, signature.inputs.len()));
            }
        }
    }
    impl<'ast> Visit<'ast> for Signatures {
        fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
            const INTERMEDIATE: &[&str] = &[
                "Address","Source","Installation","Record","Declaration","Location","Tile","Placement","Registration","Registrations","Runtimes","Image","Readiness","Launch","Wiring","Connections","Internal","Incoming","Inbox","Request","Outcome","Approved","Kind","Execution","Operation","Tracked","Active","Operations","Startup","Flow","Activity","Bound","Shutoff","Output","Mounts","Faces","Book","Buffer","Current","CurrentTip","Response","Ready","Epoch","Changed","Running","Tips","Tip","Ack","LateGuests","Outboxes","Hit","Selected","Settling","Judgment","Membership","BindingRequest","Selection","Subscription",
            ];
            if INTERMEDIATE.contains(&item.ident.to_string().as_str()) && item.fields.len() > 3 {
                self.violations.push(format!("{} has {} fields", item.ident, item.fields.len()));
            }
            visit::visit_item_struct(self, item);
        }
        fn visit_signature(&mut self, signature: &'ast Signature) {
            self.check(signature);
            visit::visit_signature(self, signature);
        }
    }
    fn inspect(path: &Path, violations: &mut Vec<String>) -> usize {
        let mut count = 0;
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !path.join("Cargo.toml").is_file() { count += inspect(&path, violations); }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = fs::read_to_string(&path).unwrap();
                let syntax = syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                let mut signatures = Signatures::default();
                signatures.visit_file(&syntax);
                count += signatures.count;
                violations.extend(signatures.violations.into_iter().map(|v| format!("{}: {v}", path.display())));
            }
        }
        count
    }
    #[test]
    fn every_system_implementation_function_has_at_most_three_parameters() {
        let mut violations = Vec::new();
        let count = inspect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system"), &mut violations);
        assert!(count > 100, "System source tree was not inspected");
        assert!(violations.is_empty(), "{}", violations.join("\n"));
        println!("checked {count} System signatures, including receivers");
    }
}

#[cfg(test)]
mod boundaries {
    use std::{fs, path::Path};
    use syn::visit::{self, Visit};
    #[derive(Default)]
    struct References(Vec<String>);
    impl<'a> Visit<'a> for References {
        fn visit_path(&mut self, path: &'a syn::Path) {
            self.0.push(path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().join("::"));
            visit::visit_path(self, path);
        }
        fn visit_item_use(&mut self, item: &'a syn::ItemUse) {
            fn collect(tree: &syn::UseTree, prefix: &str, out: &mut Vec<String>) {
                match tree {
                    syn::UseTree::Path(path) => collect(&path.tree, &format!("{prefix}{}::", path.ident), out),
                    syn::UseTree::Group(group) => for tree in &group.items { collect(tree, prefix, out); },
                    syn::UseTree::Name(name) => out.push(format!("{prefix}{}", name.ident)),
                    syn::UseTree::Rename(name) => out.push(format!("{prefix}{}", name.ident)),
                    syn::UseTree::Glob(_) => out.push(prefix.trim_end_matches("::").to_owned()),
                }
            }
            collect(&item.tree, "", &mut self.0);
        }
    }
    fn references(root: &Path, out: &mut References) {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { references(&path, out); }
            else if path.extension().is_some_and(|ext| ext == "rs") {
                out.visit_file(&syn::parse_file(&fs::read_to_string(path).unwrap()).unwrap());
            }
        }
    }
    #[derive(Default)]
    struct LoaderDependencies(Vec<String>);
    impl<'a> Visit<'a> for LoaderDependencies {
        fn visit_path(&mut self, path: &'a syn::Path) {
            let names: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            if names.windows(2).any(|pair| pair[0] == "system" && matches!(pair[1].as_str(), "control" | "identity" | "run")) {
                self.0.push(names.join("::"));
            }
            visit::visit_path(self, path);
        }
    }
    #[test]
    fn system_supplies_use_the_single_supply_acquisition() {
        let supplies = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/app/supplies.rs")).unwrap();
        assert!(supplies.contains("let (lease, delivery) = self.acquire(task, binding)?"));
        assert_eq!(supplies.matches(".source.acquire(").count(), 1);
    }
    #[test]
    fn instance_retirement_precedes_cleanup() {
        let hooks = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/assembly/hooks.rs")).unwrap();
        let retire = hooks.split("let mut retire = Schedule::sequence();").nth(1).unwrap().split("Ok(alloc::vec!").next().unwrap();
        let order = ["\"retirement\"","\"identity.exports\"","\"namespace\"","\"constructor\"","\"claim\"","\"supply\"","\"identity\"","\"bootstrap\"","\"reclaim\""];
        let mut at = 0;
        for step in order {
            let found = retire[at..].find(step).map(|offset| at + offset);
            assert!(found.is_some(), "missing retirement step {step}");
            at = found.unwrap() + step.len();
        }
    }
    #[test]
    fn static_pause_and_resume_target_the_same_whole_team() {
        let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/control");
        let pause = fs::read_to_string(control.join("lifecycle/debark.rs")).unwrap();
        assert!(pause.contains("team: Some(team)"));
        assert!(pause.contains("env::unit::debark_team(team)"));
        assert!(!pause.contains("debark_task("));
        assert!(pause.contains("env::UnitFail::Busy"));
        assert!(pause.contains("Ok(Progress::Pending)"));
        let source = fs::read_to_string(control.join("unit/mod.rs")).unwrap();
        let resume = source.split("fn resume(").nth(1).unwrap().split("pub(crate) fn task(").next().unwrap();
        assert!(resume.contains("team: Some(team)"));
        assert!(resume.contains("env::unit::embark_team(team)"));
        assert!(!resume.contains("embark_task("));
    }
    #[test]
    fn control_state_is_private_and_assembly_owns_operation_queue() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        for (path, name) in [("control/unit/mod.rs", "Control")] {
            let syntax = syn::parse_file(&fs::read_to_string(system.join(path)).unwrap()).unwrap();
            let state = syntax.items.iter().find_map(|item| match item { syn::Item::Struct(item) if item.ident == name => Some(item), _ => None }).unwrap();
            assert!(!state.fields.is_empty());
            for field in &state.fields {
                match &field.vis {
                    syn::Visibility::Inherited => {},
                    syn::Visibility::Restricted(vis) => assert_eq!(vis.path.segments.iter().map(|p| p.ident.to_string()).collect::<Vec<_>>(), ["crate", "system", "control"], "{name} exposes mutable state outside Control"),
                    _ => panic!("{name} exposes public mutable state"),
                }
            }
        }
        let assembly = fs::read_to_string(system.join("assembly.rs")).unwrap();
        assert!(assembly.contains("work: WorkQueue<Work>"));
        assert!(fs::read_to_string(system.join("assembly/work.rs")).unwrap().contains("struct WorkQueue"));
        assert!(!assembly.contains("struct HookProgress"));
        assert!(assembly.contains("Phase"));
        assert!(!fs::read_to_string(system.join("control/instance/state.rs")).unwrap().contains("pub hook:"));
    }
    #[test]
    fn lifecycle_dispatch_does_not_own_task_state_or_compensation() {
        let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/control");
        for old in ["serve/lifecycle", "serve/driver.rs", "serve/schedule.rs"] { assert!(!control.join(old).exists()); }
        let mut paths = References::default();
        let source = fs::read_to_string(control.join("lifecycle/dispatch.rs")).unwrap();
        paths.visit_file(&syn::parse_file(&source).unwrap());
        assert!(!paths.0.iter().any(|path| path.ends_with("::Control") || path.ends_with("::State") || path.ends_with("::Slot") || path.starts_with("env::")));
        #[derive(Default)]
        struct StateFields(Vec<String>);
        impl<'a> Visit<'a> for StateFields {
            fn visit_expr_field(&mut self, field: &'a syn::ExprField) {
                if let syn::Member::Named(name) = &field.member {
                    if ["table", "instances", "loader", "pending", "failure", "deadline"].contains(&name.to_string().as_str()) { self.0.push(name.to_string()); }
                }
                visit::visit_expr_field(self, field);
            }
        }
        let mut fields = StateFields::default(); fields.visit_file(&syn::parse_file(&source).unwrap());
        assert!(fields.0.is_empty());
    }
    #[test]
    fn control_request_handlers_do_not_own_registration_or_instance_effects() {
        let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/control");
        for old in ["unit.rs","instances.rs","create.rs","fixture.rs","observe.rs","start.rs","source.rs","task.rs","material.rs","reap.rs","hook.rs"] { assert!(!control.join("endpoint").join(old).exists()); }
        for domain in ["unit", "instance"] {
            let mut paths = References::default(); references(&control.join(domain), &mut paths);
            assert!(!paths.0.iter().any(|path| path.contains("control::serve")));
        }
        let mut paths = References::default(); references(&control.join("endpoint"), &mut paths);
        assert!(!paths.0.iter().any(|path| (path.starts_with("env::unit::") && path != "env::unit::self_id") || path.starts_with("env::room::")));
        #[derive(Default)]
        struct StateFields(Vec<String>);
        impl<'a> Visit<'a> for StateFields {
            fn visit_expr_field(&mut self, field: &'a syn::ExprField) {
                if let syn::Member::Named(name) = &field.member {
                    if ["table", "instances", "loader"].contains(&name.to_string().as_str()) { self.0.push(name.to_string()); }
                }
                visit::visit_expr_field(self, field);
            }
        }
        for name in ["request.rs", "instance.rs"] {
            let source = fs::read_to_string(control.join("endpoint").join(name)).unwrap();
            let mut fields = StateFields::default(); fields.visit_file(&syn::parse_file(&source).unwrap());
            assert!(fields.0.is_empty());
        }
    }
    #[test]
    fn loader_does_not_depend_on_lifecycle_identity_or_composition() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        let mut dependencies = LoaderDependencies::default();
        for name in ["cache.rs","build.rs","mapping.rs","source.rs","task.rs","image.rs"] {
            dependencies.visit_file(&syn::parse_file(&fs::read_to_string(system.join("loader").join(name)).unwrap()).unwrap());
        }
        assert!(dependencies.0.is_empty());
    }
    #[test]
    fn identity_authority_does_not_own_control_installation_state() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let identity = repo.join("server/src/system/identity");
        assert!(!identity.join("client").exists());
        let mut paths = References::default();
        for core in ["book", "service"] { references(&identity.join(core), &mut paths); }
        paths.visit_file(&syn::parse_file(&fs::read_to_string(identity.join("mod.rs")).unwrap()).unwrap());
        assert!(!paths.0.iter().any(|path| path.contains("control::identity") || path.ends_with("::Roster")));
        let source = fs::read_to_string(repo.join("server/src/system/control/identity.rs")).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let roster = syntax.items.iter().find_map(|item| match item { syn::Item::Struct(item) if item.ident == "Roster" => Some(item), _ => None }).expect("Control installation state");
        assert!(matches!(&roster.vis, syn::Visibility::Restricted(vis) if vis.path.is_ident("crate")));
        assert!(roster.fields.iter().all(|field| matches!(field.vis, syn::Visibility::Inherited)));
    }
    #[test]
    fn public_clients_do_not_export_wire_data_or_server_bindings() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for domain in ["loader","identity","operator","control"] {
            let path = repo.join("client/src").join(domain).join("mod.rs");
            let syntax = syn::parse_file(&fs::read_to_string(path).unwrap()).unwrap();
            for item in syntax.items {
                match item {
                    syn::Item::Mod(item) => if matches!(item.vis, syn::Visibility::Public(_)) {
                        assert!(!["client","rpc","exchange","frame","marks","grant"].contains(&item.ident.to_string().as_str()));
                    },
                    syn::Item::Use(item) => if matches!(item.vis, syn::Visibility::Public(_)) {
                        let mut paths = References::default(); paths.visit_item_use(&item);
                        assert!(paths.0.iter().all(|path| !path.starts_with("system_api::") || path == "system_api::loader::Built"));
                    },
                    _ => {},
                }
            }
        }
        for path in ["identity/service/face.rs","control/endpoint/request.rs","control/endpoint/instance.rs","loader/service/answer.rs"] {
            let source = fs::read_to_string(repo.join("server/src/system").join(path)).unwrap();
            assert!(!source.contains("system_client::"));
        }
    }
    #[test]
    fn system_components_have_named_ownership_and_no_legacy_classification() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        for name in ["app","control","identity","operator","loader","launch","assembly","supply"] { assert!(system.join(name).is_dir()); }
        assert!(!system.join("publication/mod.rs").exists());
        for old in ["run","common","boot.rs","life.rs","control/core","control/serve","identity/core","identity/serve","operator/core","operator/serve","operator/client.rs","loader/core","loader/serve"] { assert!(!system.join(old).exists()); }
        for name in ["account.rs","resource.rs","publication","install.rs","run.rs"] { assert!(!system.join("control/endpoint").join(name).exists()); }
        let mut paths = References::default();
        for name in ["install.rs","schedule.rs"] { paths.visit_file(&syn::parse_file(&fs::read_to_string(system.join("app").join(name)).unwrap()).unwrap()); }
        assert!(!paths.0.iter().any(|path| ["Request","Decision","Dispatch","CurrentTip","Inbox","Outcome","Runtimes","Registrations"].iter().any(|private| path.ends_with(&format!("::{private}")))));
    }
    #[test]
    fn control_uses_injected_cross_capability_hooks() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        let mut paths = References::default(); references(&system.join("control"), &mut paths);
        assert!(!paths.0.iter().any(|path| path.starts_with("crate::system::launch") || path.starts_with("crate::system::publication") || path.starts_with("crate::system::account")));
        let source = fs::read_to_string(system.join("control/lifecycle/startup.rs")).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let startup = syntax.items.iter().find_map(|item| match item { syn::Item::Struct(item) if item.ident == "Startup" => Some(item), _ => None }).unwrap();
        assert!(startup.fields.iter().all(|field| matches!(field.vis, syn::Visibility::Inherited)));
        let app = syn::parse_file(&fs::read_to_string(system.join("app/mod.rs")).unwrap()).unwrap();
        assert!(app.items.iter().any(|item| matches!(item, syn::Item::Enum(item) if item.ident == "Fault")));
    }
    #[test]
    fn system_uses_declarations_not_account_service_implementation() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        assert!(!system.join("app/account.rs").exists());
        let mut paths = References::default(); references(&system, &mut paths);
        assert!(!paths.0.iter().any(|path| path.starts_with("crate::service::account") || path.starts_with("programs::service::account") || path.starts_with("account_api::") || path.starts_with("account_client::")));
        let startup = fs::read_to_string(system.join("app/schedule.rs")).unwrap();
        assert!(!startup.contains("account.") && !startup.contains("account::"));
    }
    #[test]
    fn functional_owners_are_private_and_runtime_adapts_them() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        for old in ["operator/service","client/src/operator/client","control/service"] { assert!(!system.join(old).exists()); }
        for (file, owners) in [("operator/session.rs", &["Guest","Desk"][..]),("operator/watch.rs", &["Subscription","Subscriber","Watchers"][..]),("control/unit/service.rs", &["Service"][..]),("control/endpoint/mod.rs", &["Entries"][..])] {
            let syntax = syn::parse_file(&fs::read_to_string(system.join(file)).unwrap()).unwrap();
            for name in owners {
                let owner = syntax.items.iter().find_map(|item| match item { syn::Item::Struct(item) if item.ident == *name => Some(item), _ => None }).unwrap_or_else(|| panic!("missing owner {file}::{name}"));
                assert!(owner.fields.iter().all(|field| matches!(field.vis, syn::Visibility::Inherited)));
            }
            let mut paths = References::default(); paths.visit_file(&syntax);
            assert!(!paths.0.iter().any(|path| path.starts_with("crate::system::operator::runtime::")));
        }
        assert!(!fs::read_to_string(system.join("operator/connection.rs")).unwrap().contains(".pull(&mut bytes, budget.remaining())"));
        assert!(system.join("operator/runtime/mod.rs").is_file());
        assert!(!system.join("operator/runtime/run.rs").exists());
    }
    #[test]
    fn request_servers_do_not_depend_on_supervisor_or_client_implementations() {
        let system = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system");
        for domain in ["identity","operator","control"] {
            let mut paths = References::default();
            let adapter = match domain { "operator" => "runtime", "control" => "endpoint", _ => "service" };
            references(&system.join(domain).join(adapter), &mut paths);
            for path in paths.0 {
                assert!(!path.starts_with("crate::system::app::execute") && !path.starts_with("crate::system::app::boot"));
                if domain != "control" {
                    assert!(!path.starts_with("crate::system::control"));
                    assert!(!path.starts_with(&format!("crate::system::{domain}::client")));
                }
            }
        }
    }
    #[test]
    fn loader_frame_and_shutdown_keep_their_distinct_steps() {
        #[derive(Default)]
        struct Steps(Vec<String>);
        impl<'a> Visit<'a> for Steps {
            fn visit_expr_method_call(&mut self, call: &'a syn::ExprMethodCall) {
                if call.method == "system" {
                    if let Some(syn::Expr::Lit(literal)) = call.args.first() {
                        if let syn::Lit::Str(name) = &literal.lit { self.0.push(name.value()); }
                    }
                }
                visit::visit_expr_method_call(self, call);
            }
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system/loader/service/plan.rs");
        let syntax = syn::parse_file(&fs::read_to_string(path).unwrap()).unwrap();
        let mut checked = 0;
        for item in syntax.items {
            if let syn::Item::Fn(function) = item {
                let expected: &[&str] = match function.sig.ident.to_string().as_str() { "frame" => &["receive"], "shutdown" => &["withdraw","close"], _ => continue };
                let mut steps = Steps::default(); steps.visit_block(&function.block);
                assert_eq!(steps.0, expected);
                checked += 1;
            }
        }
        assert_eq!(checked, 2);
    }
    #[test]
    fn system_client_dependencies_stay_one_way() {
        fn dependencies(path: &Path) -> Vec<String> {
            let source = fs::read_to_string(path).unwrap();
            let mut result = Vec::new(); let mut section = false;
            for line in source.lines().map(str::trim) {
                if line.starts_with('[') { section = line.contains("dependencies"); }
                else if section { if let Some((name, _)) = line.split_once('=') { result.push(name.trim().to_owned()); } }
            }
            result
        }
        let clients = dependencies(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../client/Cargo.toml"));
        assert!(clients.iter().any(|name| name == "system-api"));
        assert!(!clients.iter().any(|name| name == "protocol" || name == "programs"));
        let mut paths = References::default();
        references(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../client/src"), &mut paths);
        assert!(!paths.0.iter().any(|path| path.starts_with("protocol::")));
    }
    #[test]
    fn system_runtime_does_not_own_account_or_legacy_modules() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(!repo.join("server/src/system/account").exists());
        let control = fs::read_to_string(repo.join("server/src/system/control/unit/mod.rs")).unwrap();
        assert!(!control.contains("loader:"));
        let operator = fs::read_to_string(repo.join("server/src/system/operator/runtime/authority.rs")).unwrap();
        assert!(!operator.contains("hub_api") && !operator.contains("Machine"));
        assert!(!repo.join("client/src/control/publication.rs").exists());
        assert!(!repo.join("server/src/system/launch/activation.rs").exists());
        assert!(!repo.join("server/src/system/control/unit/material.rs").exists());
    }
    #[test]
    fn system_runtime_does_not_name_daemon() {
        fn names_daemon(root: &Path) -> bool {
            for entry in fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { if names_daemon(&path) { return true; } }
                else if path.extension().is_some_and(|ext| ext == "rs") && fs::read_to_string(path).unwrap().contains("\"daemon\"") { return true; }
            }
            false
        }
        assert!(!names_daemon(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server/src/system")));
    }
}
