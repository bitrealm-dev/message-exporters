use sms_backup_plus_exporter::dedupe_eml;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_eml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|x| x.to_str()) == Some("eml") {
                out.push(path);
            }
        }
    }
    walk(dir, &mut out);
    out
}

#[test]
fn dedupes_duplicate_flats_to_one_output() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    let copy_a = input.path().join("export_a");
    let copy_b = input.path().join("export_b").join("Sent");
    fs::create_dir_all(&copy_a).unwrap();
    fs::create_dir_all(&copy_b).unwrap();
    fs::copy(
        fixtures.join("flat_received.eml"),
        copy_a.join("flat_received.eml"),
    )
    .unwrap();
    fs::copy(
        fixtures.join("flat_received.eml"),
        copy_b.join("flat_received.eml"),
    )
    .unwrap();
    fs::copy(fixtures.join("archive.eml"), copy_a.join("archive.eml")).unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();

    assert_eq!(report.flat_seen, 2, "errors: {:?}", report.errors);
    assert_eq!(report.unique_flat, 1, "errors: {:?}", report.errors);
    assert_eq!(report.copied, 1, "errors: {:?}", report.errors);
    assert_eq!(report.duplicates_dropped, 1);
    assert_eq!(report.archive_eml, 1);
    // archive.eml has 2 messages that do not overlap the flat fixture
    assert_eq!(report.archive_only, 2);
    assert_eq!(report.archive_generated, 2);
    assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

    let log_path = report.log_path.expect("dedupe.log path");
    assert!(log_path.exists());
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("COPY   "), "log missing COPY:\n{log}");
    assert!(
        log.contains("DROP   ") || log.contains("REPLACE "),
        "log missing DROP/REPLACE:\n{log}"
    );
    assert!(log.contains("ARCHIVE "), "log missing ARCHIVE:\n{log}");
    assert!(log.contains("GENERATED "), "log missing GENERATED:\n{log}");
    assert!(
        log.contains("skipped_not_sms:"),
        "log missing skipped_not_sms:\n{log}"
    );
    assert!(log.contains("## summary"), "log missing summary:\n{log}");

    let eml_files = collect_eml_files(output.path());
    assert_eq!(eml_files.len(), 3); // 1 copied flat + 2 generated archive-only
    assert!(
        eml_files.iter().all(|p| {
            p.parent()
                .and_then(|y| y.file_name())
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() == 4 && n.chars().all(|c| c.is_ascii_digit()))
        }),
        "expected year subdirs, got: {eml_files:?}"
    );
}

#[test]
fn same_smssync_id_different_messages_both_kept() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    fs::copy(
        fixtures.join("flat_smssync_276_alex.eml"),
        input.path().join("alex.eml"),
    )
    .unwrap();
    fs::copy(
        fixtures.join("flat_smssync_276_sam.eml"),
        input.path().join("sam.eml"),
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();

    assert_eq!(report.flat_seen, 2, "errors: {:?}", report.errors);
    assert_eq!(report.unique_flat, 2, "errors: {:?}", report.errors);
    assert_eq!(report.copied, 2);
    assert_eq!(report.duplicates_dropped, 0);

    let eml_files = collect_eml_files(output.path());
    assert_eq!(eml_files.len(), 2);
}

#[test]
fn archive_message_overlaps_matching_flat_via_content_key() {
    use chrono::{Local, NaiveDateTime, TimeZone};

    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    let naive = NaiveDateTime::parse_from_str("2020-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let local_ts = Local
        .from_local_datetime(&naive)
        .single()
        .unwrap()
        .timestamp();
    let ms = local_ts * 1000;

    fs::write(
        input.path().join("archive.eml"),
        b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
Check this\r\n",
    )
    .unwrap();

    fs::write(
        input.path().join("flat.eml"),
        format!(
            "From: me@example.com\r\n\
To: 4075551234@sms-backup-plus.local\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 2\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: {ms}\r\n\
X-smssync-id: 999\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Check this\r\n"
        ),
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();

    assert_eq!(report.flat_seen, 1, "errors: {:?}", report.errors);
    assert_eq!(report.archive_eml, 1);
    assert_eq!(report.archive_overlaps, 1, "errors: {:?}", report.errors);
    assert_eq!(report.archive_only, 0);
    assert_eq!(report.archive_generated, 0);
    assert_eq!(report.copied, 1);

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(
        log.contains("archive_overlaps: 1"),
        "expected overlap summary in log:\n{log}"
    );
}

#[test]
fn archive_overlaps_flat_despite_millisecond_mismatch() {
    use chrono::{Local, NaiveDateTime, TimeZone};

    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    let naive = NaiveDateTime::parse_from_str("2020-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let local_ts = Local
        .from_local_datetime(&naive)
        .single()
        .unwrap()
        .timestamp();
    // Flat has sub-second ms; archive body is whole seconds only.
    let ms = local_ts * 1000 + 488;

    fs::write(
        input.path().join("archive.eml"),
        b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
Will do\r\n",
    )
    .unwrap();

    fs::write(
        input.path().join("flat.eml"),
        format!(
            "From: me@example.com\r\n\
To: 4075551234@sms-backup-plus.local\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 2\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: {ms}\r\n\
X-smssync-id: 999\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Will do\r\n"
        ),
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();

    assert_eq!(report.archive_overlaps, 1, "errors: {:?}", report.errors);
    assert_eq!(report.archive_only, 0);
    assert_eq!(report.archive_generated, 0);
    assert_eq!(report.copied, 1);

    let eml_files = collect_eml_files(output.path());
    assert_eq!(eml_files.len(), 1);
    assert!(
        eml_files[0]
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("_sent_14075551234_")),
        "expected single-underscore phone stem, got {:?}",
        eml_files[0]
    );
}

#[test]
fn archive_only_generates_parseable_flat() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    fs::write(
        input.path().join("archive.eml"),
        b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
Only in archive\r\n",
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();
    assert_eq!(report.archive_only, 1);
    assert_eq!(report.archive_generated, 1);
    assert_eq!(report.copied, 0);

    let eml_files = collect_eml_files(output.path());
    assert_eq!(eml_files.len(), 1);

    let body = fs::read_to_string(&eml_files[0]).unwrap();
    assert!(
        body.contains("Only in archive"),
        "generated body missing text:\n{body}"
    );
    assert!(
        body.contains("X-smssync-id: gen-"),
        "expected synthetic smssync id:\n{body}"
    );
    assert!(
        body.contains("X-smssync-type: 2"),
        "expected sent type for Me:\n{body}"
    );

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(log.contains("GENERATED "), "log:\n{log}");
}

#[test]
fn unknown_phone_archive_goes_to_junk() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    fs::write(
        input.path().join("archive.eml"),
        b"From: someone@example.com\r\n\
To: me@example.com\r\n\
Subject: SMS archive Mystery\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Mystery\r\n\
2020-01-01 12:00:00 - Me\r\n\
No phone here\r\n",
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();
    assert_eq!(report.archive_generated, 1);
    assert_eq!(report.archive_generated_junk, 1);

    let junk = output.path().join("junk");
    assert!(junk.is_dir());
    let junk_files = collect_eml_files(&junk);
    assert_eq!(junk_files.len(), 1);
    assert!(
        junk_files[0]
            .strip_prefix(&junk)
            .unwrap()
            .components()
            .count()
            >= 2,
        "expected junk/{{year}}/…, got {:?}",
        junk_files[0]
    );

    let root_eml: Vec<_> = fs::read_dir(output.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("eml"))
        .collect();
    assert!(root_eml.is_empty());

    let unresolved = fs::read_to_string(output.path().join("junk/unresolved_names.txt")).unwrap();
    assert!(
        unresolved.lines().any(|l| l == "Mystery"),
        "expected Mystery in unresolved list:\n{unresolved}"
    );
    assert_eq!(report.unresolved_names, 1);
}

#[test]
fn unknown_phone_flat_copied_to_junk() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    fs::write(
        input.path().join("flat.eml"),
        b"From: Siminn@unknown.email\r\n\
To: me@example.com\r\n\
Subject: SMS with Siminn\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: Siminn\r\n\
X-smssync-date: 1609459200000\r\n\
X-smssync-id: 42\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
You have 1 new voicemail\r\n",
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();
    assert_eq!(report.flat_unknown_junk, 1);
    assert_eq!(report.copied, 0);
    assert_eq!(report.skipped_unparseable, 0);
    assert_eq!(report.unresolved_names, 1);

    let junk_files = collect_eml_files(&output.path().join("junk"));
    assert_eq!(junk_files.len(), 1);
    let unresolved = fs::read_to_string(output.path().join("junk/unresolved_names.txt")).unwrap();
    assert!(unresolved.lines().any(|l| l == "Siminn"));

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(log.contains("JUNK   "), "log:\n{log}");
    assert!(log.contains("(junk/ unknown phone)"), "log:\n{log}");
}

#[test]
fn short_code_address_is_kept() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    fs::write(
        input.path().join("flat.eml"),
        b"From: <73737@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS with 73737\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: 73737\r\n\
X-smssync-date: 1609459200000\r\n\
X-smssync-id: 100\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Your code is 1234\r\n",
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();
    assert_eq!(report.flat_seen, 1, "errors: {:?}", report.errors);
    assert_eq!(report.copied, 1);
    assert_eq!(report.skipped_unparseable, 0);

    let eml_files = collect_eml_files(output.path());
    assert_eq!(eml_files.len(), 1);
    assert!(
        eml_files[0]
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("_recv_73737_")),
        "got {:?}",
        eml_files[0]
    );
}

#[test]
fn unparseable_flat_copied_to_unparseable_dir() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();

    // SMS-shaped headers but no usable address/name → parse returns None.
    fs::write(
        input.path().join("broken.eml"),
        b"From: nobody@example.com\r\n\
To: me@example.com\r\n\
Subject: SMS with \r\n\
X-smssync-type: 1\r\n\
X-smssync-address: \r\n\
X-smssync-date: 1609459200000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
orphan body\r\n",
    )
    .unwrap();

    let report = dedupe_eml(&[input.path()], output.path(), "5555550100", &[], None, None, false).unwrap();
    assert_eq!(report.skipped_unparseable, 1);
    assert_eq!(report.copied, 0);

    let dir = report.unparseable_dir.expect("unparseable dir");
    assert!(dir.is_dir());
    let files = collect_eml_files(&dir);
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("broken.eml") || files[0].file_name().is_some());

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(log.contains("UNPARSEABLE "), "log:\n{log}");
}

#[test]
fn contacts_csv_resolves_unknown_archive_phone() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let contacts_dir = tempfile::tempdir().unwrap();
    let contacts = contacts_dir.path().join("contacts.csv");
    fs::write(
        &contacts,
        "phones,first_name,last_name\n\
15555550999,Mystery,Person\n",
    )
    .unwrap();

    fs::write(
        input.path().join("archive.eml"),
        b"From: someone@example.com\r\n\
To: me@example.com\r\n\
Subject: SMS archive Mystery Person\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Mystery Person\r\n\
2020-01-01 12:00:00 - Me\r\n\
Resolved via contacts\r\n",
    )
    .unwrap();

    let report = dedupe_eml(
        &[input.path()],
        output.path(),
        "5555550100",
        &[],
        Some(&contacts),
        None,
        false,
    )
    .unwrap();
    assert_eq!(report.contacts_resolved, 1);
    assert_eq!(report.archive_generated, 1);
    assert_eq!(report.archive_generated_junk, 0);
    assert_eq!(report.unresolved_names, 0);

    let root_eml = collect_eml_files(output.path());
    assert_eq!(root_eml.len(), 1);
    let body = fs::read_to_string(&root_eml[0]).unwrap();
    assert!(body.contains("X-smssync-address: +15555550999") || body.contains("15555550999"));
    assert!(
        root_eml[0]
            .parent()
            .and_then(|y| y.file_name())
            .and_then(|n| n.to_str())
            == Some("2020"),
        "expected under 2020/, got {:?}",
        root_eml[0]
    );

    let unresolved = fs::read_to_string(output.path().join("junk/unresolved_names.txt")).unwrap();
    assert!(unresolved.trim().is_empty());

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(
        log.contains("CONTACT Mystery Person -> 5555550999"),
        "log:\n{log}"
    );
}

#[test]
fn name_mapping_aliases_eml_name_for_contacts_lookup() {
    let input = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let contacts = cfg.path().join("contacts.csv");
    let mapping = cfg.path().join("mapping.csv");
    fs::write(
        &contacts,
        "phones,first_name,last_name\n\
15555550888,Casey,Proper\n",
    )
    .unwrap();
    fs::write(
        &mapping,
        "correct_name,incorrect_name\n\
Casey Proper,Casey Typo\n",
    )
    .unwrap();

    fs::write(
        input.path().join("archive.eml"),
        b"From: someone@example.com\r\n\
To: me@example.com\r\n\
Subject: SMS archive Casey Typo\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Casey Typo\r\n\
2020-01-01 12:00:00 - Me\r\n\
Mapped then resolved\r\n",
    )
    .unwrap();

    let report = dedupe_eml(
        &[input.path()],
        output.path(),
        "5555550100",
        &[],
        Some(&contacts),
        Some(&mapping),
        false,
    )
    .unwrap();
    assert_eq!(report.names_mapped, 1);
    assert_eq!(report.contacts_resolved, 1);
    assert_eq!(report.archive_generated_junk, 0);

    let log = fs::read_to_string(report.log_path.unwrap()).unwrap();
    assert!(
        log.contains("MAP     Casey Typo -> Casey Proper"),
        "log:\n{log}"
    );
    assert!(
        log.contains("CONTACT Casey Proper -> 5555550888"),
        "log:\n{log}"
    );
}
