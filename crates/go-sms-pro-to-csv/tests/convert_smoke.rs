use go_sms_pro_to_csv::convert_export;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

#[test]
fn convert_smoke_writes_csv_not_json() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    assert!(input.is_dir(), "missing fixture: {}", input.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let report = convert_export(input.as_path(), tmp.path(), &["+15555550100".into()])
        .expect("convert_export should succeed");
    assert!(report.conversations >= 1);
    assert!(report.xml_messages >= 2);

    let mut csv_files: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    csv_files.sort();
    assert!(!csv_files.is_empty(), "expected at least one .csv");

    let json_count = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .count();
    assert_eq!(json_count, 0);

    let mut contents = String::new();
    File::open(&csv_files[0])
        .unwrap()
        .read_to_string(&mut contents)
        .unwrap();
    let header = contents.lines().next().unwrap();
    assert!(header.contains("chat_identifier"));
    assert!(header.contains("direction"));
    assert!(header.contains("export_source"));
    assert!(header.contains("source_kind"));
    assert!(header.contains("xml_fields_json"));
    assert!(!header.contains("participants_json"));
    assert!(!header.contains("read_receipt"));
    assert!(!header.contains("tapbacks_json"));
    assert!(contents.contains("go-sms-pro"));
    assert!(contents.contains("incoming") || contents.contains("outgoing"));
    assert!(contents.contains("smoke"));
}
