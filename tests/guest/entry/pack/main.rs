use unit_contract::{
    bundle::{self, File, Kind, Unit},
    document as d,
};
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3);
    let capsule = std::fs::read(&args[0]).unwrap();
    let image = std::fs::read(&args[1]).unwrap();
    let declaration = d::v2::Unit::<String> {
        schema: 2,
        id: "system".into(),
        loader: d::Loader {
            image: "system-bin".into(),
            space: d::Space::Supervisor,
            ..Default::default()
        },
        ..Default::default()
    };
    let record = unit_contract::record::v2::encode(&declaration).unwrap();
    let packed = bundle::pack(
        &[Unit {
            id: "system",
            record: &record,
        }],
        &[File {
            kind: Kind::Supervisor,
            name: "system-bin",
            bytes: &image,
        }],
        0,
        0,
        &capsule,
    )
    .unwrap();
    std::fs::write(&args[2], packed).unwrap();
}
