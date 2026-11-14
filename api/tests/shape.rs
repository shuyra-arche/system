use std::{fs, path::Path};

#[test]
fn provider_api_keeps_one_way_dependencies() {
    fn dependencies(path: &Path) -> Vec<String> {
        let source = fs::read_to_string(path).unwrap();
        let mut result = Vec::new();
        let mut dependency_section = false;
        for line in source.lines().map(str::trim) {
            if line.starts_with('[') {
                dependency_section = line.contains("dependencies");
            } else if dependency_section && let Some((name, _)) = line.split_once('=') {
                result.push(name.trim().to_owned());
            }
        }
        result
    }
    let api = dependencies(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    assert!(
        api.iter()
            .all(|name| ["kernel-api", "weave"].contains(&name.as_str()))
    );
}

#[test]
fn api_has_no_control_publication_module() {
    let api = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!api.join("src/control/publication.rs").exists());
}

#[test]
fn sdk_separates_read_discovery_from_entry_capabilities() {
    use system_api::{Path, PendId, operator, value::Request};
    use weave::{Codec, Contract};
    let request = Request::<operator::operations::Request, 3> {
        reply: PendId::new(1),
        body: operator::operations::Request::Seek(operator::Road {
            path: Path::new("service").unwrap(),
        }),
    };
    let mut bytes = [0; 512];
    let used = request.encode(&mut bytes).unwrap();
    type AuthorityRequest = <operator::authority::Call as Contract>::Request;
    assert!(AuthorityRequest::decode_exact(&bytes[..used]).is_none());
}
