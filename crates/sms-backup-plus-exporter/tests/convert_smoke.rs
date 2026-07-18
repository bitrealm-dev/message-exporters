use sms_backup_plus_exporter::convert_export;
use std::fs;
use std::path::PathBuf;

#[test]
fn converts_fixtures_in_temp_dir() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in ["flat_received.eml", "archive.eml"] {
        fs::copy(fixtures.join(name), input.path().join(name)).unwrap();
    }

    let report =
        convert_export(&[input.path()], output.path(), &["5555550100".into()], &[], None, None, false).unwrap();

    assert_eq!(report.flat_eml, 1, "errors: {:?}", report.errors);
    assert_eq!(report.archive_eml, 1, "errors: {:?}", report.errors);
    assert_eq!(report.messages, 3, "errors: {:?}", report.errors);
    assert_eq!(report.conversations, 1);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

    let json_files: Vec<_> = fs::read_dir(output.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    assert_eq!(json_files.len(), 1);
}
