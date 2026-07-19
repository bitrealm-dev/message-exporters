use imazing_to_csv::convert_export;
use message_contacts::ContactsBook;
use std::fs;
use std::path::PathBuf;

#[test]
fn convert_messages_with_imazing_contacts() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let messages = fixture.join("messages.csv");
    let contacts = fixture.join("contacts.csv");
    assert!(messages.is_file(), "missing {}", messages.display());
    assert!(contacts.is_file(), "missing {}", contacts.display());

    let book = ContactsBook::load_imazing_contacts_csv(&contacts).expect("load contacts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let report = convert_export(&messages, tmp.path(), &book, Some("UTC")).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 3);
    assert_eq!(report.unresolved_chat_phone, 0);

    let out = tmp.path().join("_13212462167.csv");
    let body = fs::read_to_string(&out).expect("read csv");
    assert!(body.contains("chat_identifier"));
    assert!(body.contains("imazing"));
    assert!(body.contains("iMazing"));
    assert!(body.contains("3.5.5"));
    assert!(body.contains("Bob McRoy"));
    assert!(body.contains("image000000.jpg"));
}
