use message_contacts::ContactsBook;
use message_csv::DateRange;
use message_exporters_core::OutputFormat;
use sms_backup_restore_exporter::convert_export;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

fn empty_contacts(dir: &tempfile::TempDir) -> ContactsBook {
    let path = dir.path().join("contacts.csv");
    let mut f = File::create(&path).unwrap();
    writeln!(f, "First Name,Last Name,Mobile Phone").unwrap();
    ContactsBook::load_imazing_contacts_csv(&path).unwrap()
}

#[test]
fn convert_export_smoke_on_sample_fixture() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    assert!(fixture.is_file(), "missing fixture: {}", fixture.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let report = convert_export(
        &fixture,
        tmp.path(),
        &["+15555550100".into()],
        &contacts,
        &DateRange::default(),
        true,
        OutputFormat::Csv,
        None,
    )
    .expect("convert_export should succeed");

    assert!(
        report.conversations >= 1,
        "expected >=1 conversations, got {}",
        report.conversations
    );

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
    assert!(header.contains("export_source"));
    assert!(header.contains("export_tool"));
    assert!(header.contains("export_tool_version"));
    assert!(header.contains("message_kind"));
    assert!(header.contains("xml_fields_json"));
    assert!(header.contains("subject"));
    assert!(!header.contains("participants_json"));
    assert!(!header.contains("read_receipt"));
    assert!(!header.contains("tapbacks_json"));
    assert!(contents.contains("sms-backup-restore"));

    let attachments = tmp.path().join("attachments");
    let mut found = false;
    if attachments.is_dir() {
        for entry in std::fs::read_dir(&attachments).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected at least one attachment file under {}",
        attachments.display()
    );
}

#[test]
fn dedupes_overlapping_xml_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let input_dir = tmp.path().join("in");
    fs::create_dir_all(&input_dir).unwrap();

    let xml = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <sms address="+15555550101" date="1400773261000" type="1" body="same text" contact_name="Sam" />
</smses>"#;
    fs::write(input_dir.join("a.xml"), xml).unwrap();
    fs::write(input_dir.join("b.xml"), xml).unwrap();

    let out = tmp.path().join("out");
    let contacts = empty_contacts(&tmp);
    let report = convert_export(
        &input_dir,
        &out,
        &["+15555550100".into()],
        &contacts,
        &DateRange::default(),
        true,
        OutputFormat::Csv,
        None,
    )
    .unwrap();
    assert_eq!(report.sms_seen, 2);
    assert_eq!(report.conversations, 1);
    assert_eq!(report.received, 1); // one row after dedupe

    let chat = out.join("+15555550101.csv");
    let body = fs::read_to_string(&chat).unwrap();
    // header + one message row (duplicate dropped)
    assert_eq!(body.lines().count(), 2);
    assert!(body.contains("same text"));
}

#[test]
fn rejects_owner_phone_without_digits() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let err = convert_export(
        &fixture,
        tmp.path(),
        &["not-a-phone".into()],
        &contacts,
        &DateRange::default(),
        true,
        OutputFormat::Csv,
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("owner phone"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn convert_export_eml_writes_conversation_folder() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    assert!(fixture.is_file(), "missing fixture: {}", fixture.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let report = convert_export(
        &fixture,
        tmp.path(),
        &["+15555550100".into()],
        &contacts,
        &DateRange::default(),
        true,
        OutputFormat::Eml,
        None,
    )
    .expect("convert_export eml should succeed");

    assert!(
        report.conversations >= 1,
        "expected >=1 conversations, got {}",
        report.conversations
    );

    let mut eml_dirs = Vec::new();
    let mut eml_files = 0usize;
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "attachments" {
            continue;
        }
        let count = fs::read_dir(&path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("eml"))
            .count();
        if count > 0 {
            eml_dirs.push(path);
            eml_files += count;
        }
    }
    assert!(
        !eml_dirs.is_empty(),
        "expected at least one conversation directory with .eml"
    );
    assert!(eml_files >= 1, "expected at least one .eml file");

    let sample = fs::read(
        fs::read_dir(&eml_dirs[0])
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|x| x.to_str()) == Some("eml"))
            .unwrap(),
    )
    .unwrap();
    let text = String::from_utf8_lossy(&sample);
    assert!(text.contains("X-ME-Export-Source: sms-backup-restore"));
    assert!(text.contains("X-ME-Guid:"));
}
