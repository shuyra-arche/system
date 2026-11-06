use initrd::ledger::{
    bundle::{self, File, Kind, Unit},
    capsule,
};
use unit::document;

fn capsule() -> Vec<u8> {
    let mut out = vec![0; capsule::PAGE * 2];
    out[..8].copy_from_slice(&capsule::MAGIC);
    out[8..10].copy_from_slice(&1u16.to_le_bytes());
    out[10] = 12;
    out[12..16].copy_from_slice(&1u32.to_le_bytes());
    out[16..24].copy_from_slice(&0x1000u64.to_le_bytes());
    let size = out.len() as u64;
    out[24..32].copy_from_slice(&size.to_le_bytes());
    for (at, value) in [
        (32, 0x1000u64),
        (40, 1),
        (48, 1),
        (56, capsule::PAGE as u64),
    ] {
        out[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    out[64..68].copy_from_slice(&5u32.to_le_bytes());
    out
}
fn document(id: &str, image: &str) -> document::Unit<String> {
    document::Unit {
        schema: 1,
        id: id.into(),
        loader: document::Loader {
            image: image.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn main() {
    let mut system = document("system", "system-bin");
    system.loader.space = document::Space::Supervisor;
    let mut provider = document("provider", "shared-bin");
    provider.control.after = vec!["known-unselected".into()];
    let mut template = document("template", "shared-bin");
    template.control.creation = document::Creation::Requested;
    let mut caller = document("caller", "shared-bin");
    caller.control.requires = vec!["provider".into(), "template".into()];
    caller.control.after = vec!["provider".into()];
    let files = [
        File {
            kind: Kind::Supervisor,
            name: "system-bin",
            bytes: b"System ELF fixture",
        },
        File {
            kind: Kind::User,
            name: "shared-bin",
            bytes: b"shared ELF fixture",
        },
    ];
    let directory = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for name in ["valid", "missing", "cycle", "mismatch"] {
        let mut docs = vec![
            system.clone(),
            caller.clone(),
            provider.clone(),
            template.clone(),
        ];
        match name {
            "missing" => docs[1].control.requires.push("missing".into()),
            "cycle" => docs[2].control.after = vec!["caller".into()],
            "mismatch" => docs[0].loader.image = "shared-bin".into(),
            _ => {}
        }
        let records: Vec<_> = docs
            .iter()
            .map(|doc| unit::record::encode(doc).unwrap())
            .collect();
        let entries: Vec<_> = docs
            .iter()
            .zip(&records)
            .map(|(doc, record)| Unit {
                id: &doc.id,
                record,
            })
            .collect();
        let bytes = bundle::pack(&entries, &files, 0, 0, &capsule()).unwrap();
        std::fs::write(directory.join(format!("{name}.img")), bytes).unwrap();
    }
}
