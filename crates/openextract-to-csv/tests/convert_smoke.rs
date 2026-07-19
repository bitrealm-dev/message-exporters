use message_contacts::ContactsBook;
use openextract_to_csv::convert_export;
use std::fs;
use std::path::PathBuf;

#[test]
fn convert_all_conversations_with_vcf() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let csv = fixture.join("all_conversations.csv");
    let vcf = fixture.join("contacts.vcf");
    assert!(csv.is_file(), "missing {}", csv.display());
    assert!(vcf.is_file(), "missing {}", vcf.display());

    let book = ContactsBook::load_vcf(&vcf).expect("load vcf");
    let tmp = tempfile::tempdir().expect("tempdir");
    let report = convert_export(&csv, tmp.path(), &book).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 2);
    assert_eq!(report.unresolved_chat_phone, 0);

    let out = tmp.path().join("_15555550122.csv");
    let body = fs::read_to_string(&out).expect("read csv");
    assert!(body.contains("chat_identifier"));
    assert!(body.contains("openextract"));
    assert!(body.contains("Sam Example"));
    assert!(body.contains("all-conversations"));
}
