//! USDZ packages: the zip container's rules, and that the layer inside is the `.usda` export.

mod support;

use limner_core::model::Document;
use limner_export::{
    export_usda_with_report, export_usdz, export_usdz_with_report, usdz_package, write_usdz,
    ExportError, ExportOptions, UsdOptions,
};
use support::stage::{stage, STAGE_LAST};
use support::usda::Usda;
use support::zip::{crc32, read_usdz};
use support::{FRAME, RATE};

#[test]
fn the_reference_crc_matches_the_published_check_value() {
    // The check value of CRC-32/ISO-HDLC, the variant zip uses.
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn the_package_holds_the_text_layer_as_its_first_and_only_file() {
    let stage = stage();
    let options = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    let extra = UsdOptions::default();
    let (bytes, report) = export_usdz_with_report(&stage.doc, &options, &extra).unwrap();
    let (text, layer_report) = export_usda_with_report(&stage.doc, &options, &extra).unwrap();

    let entries = read_usdz(&bytes);
    assert_eq!(entries.len(), 1);
    let root = &entries[0];
    // The root layer is named after the default prim.
    assert_eq!(root.name, "Stage.usda");
    assert_eq!(root.header_offset, 0, "the root layer is the first file");
    assert_eq!(root.data_offset, 64);
    assert_eq!(root.data, text.as_bytes(), "byte for byte the .usda export");
    Usda::parse(std::str::from_utf8(&root.data).unwrap());

    // The report describes the same content; only the size is the package's.
    assert_eq!(report.bytes, bytes.len() as u64);
    assert_eq!(
        limner_export::ExportReport {
            bytes: layer_report.bytes,
            ..report
        },
        layer_report
    );
    // Stored: the package is the layer plus headers and padding, nothing compressed.
    assert!(bytes.len() > text.len());
    assert!(bytes.len() < text.len() + 64 + 46 + 22 + 2 * root.name.len() + 64);
    // The same input always gives the same bytes.
    assert_eq!(export_usdz(&stage.doc, &options, &extra).unwrap(), bytes);
}

#[test]
fn an_empty_document_still_packs() {
    let bytes = export_usdz(
        &Document::empty(),
        &ExportOptions::still(0),
        &UsdOptions::default(),
    )
    .unwrap();
    let entries = read_usdz(&bytes);
    assert_eq!(entries[0].name, "Limner_scene.usda");
    Usda::parse(std::str::from_utf8(&entries[0].data).unwrap());
}

#[test]
fn every_file_starts_on_a_64_byte_boundary_whatever_the_name_lengths() {
    // Name lengths chosen so the padding takes every size from 0 to 63 bytes,
    // including the 1 to 3 bytes that are too small for an extra record.
    let names: Vec<String> = (1..=70).map(|n| "n".repeat(n) + ".bin").collect();
    let datas: Vec<Vec<u8>> = (0..names.len())
        .map(|i| {
            (0..(i * 37) % 301)
                .map(|b| (b * 7 + i) as u8)
                .collect::<Vec<u8>>()
        })
        .collect();
    let files: Vec<(&str, &[u8])> = names
        .iter()
        .zip(&datas)
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    let bytes = usdz_package(&files).unwrap();
    // `read_usdz` checks alignment, CRCs, both headers and the directory.
    let entries = read_usdz(&bytes);
    assert_eq!(entries.len(), files.len());
    let mut paddings = std::collections::BTreeSet::new();
    for (entry, (name, data)) in entries.iter().zip(&files) {
        assert_eq!(entry.name, *name);
        assert_eq!(entry.data, *data);
        assert!(entry.extra.len() < 68, "padding stays below 64 + 4 bytes");
        assert!(entry.extra.iter().skip(4).all(|b| *b == 0));
        paddings.insert(entry.extra.len());
    }
    assert!(
        paddings.contains(&0),
        "an already aligned file gets no padding"
    );
    assert!(
        !paddings.contains(&1) && !paddings.contains(&2) && !paddings.contains(&3),
        "an extra record cannot be shorter than its own header"
    );
    assert!(paddings.len() > 40, "the cases really cover the range");

    // A name with non-ASCII characters is flagged as UTF-8 and still read back.
    let bytes = usdz_package(&[("root.usda", b"#usda 1.0\n"), ("tex/é.png", b"x")]).unwrap();
    let entries = read_usdz(&bytes);
    assert_eq!(entries[1].name, "tex/é.png");
    let central = bytes.len() - 22 - (46 + "tex/é.png".len());
    assert_eq!(
        u16::from_le_bytes([bytes[central + 8], bytes[central + 9]]),
        1 << 11
    );
}

#[test]
fn the_reader_rejects_packages_that_break_the_rules() {
    let good = usdz_package(&[("a.usda", b"#usda 1.0\n" as &[u8]), ("b.bin", &[1, 2, 3])]).unwrap();
    read_usdz(&good);
    let breaks = |what: &str, change: &dyn Fn(&mut Vec<u8>)| {
        let mut bad = good.clone();
        change(&mut bad);
        let result = std::panic::catch_unwind(|| {
            read_usdz(&bad);
        });
        assert!(result.is_err(), "the reader accepted: {what}");
    };
    breaks("a flipped data byte (CRC)", &|b| b[64] ^= 0xFF);
    breaks("compression method 8", &|b| b[8] = 8);
    breaks("the encryption flag", &|b| b[6] |= 1);
    breaks("a missing end record", &|b| {
        b.pop();
    });
    breaks("data that is not aligned", &|b| {
        // Shorten the first file's padding by one byte and shift everything after it.
        let extra = u16::from_le_bytes([b[28], b[29]]);
        b[28..30].copy_from_slice(&(extra - 1).to_le_bytes());
        b.remove(40);
    });
}

#[test]
fn a_package_with_more_files_than_zip_can_count_is_refused() {
    // Without zip64 the directory counts files in 16 bits.
    let files: Vec<(&str, &[u8])> = vec![("a", &[]); 65_536];
    assert!(matches!(
        usdz_package(&files),
        Err(ExportError::FileTooLarge { format: "USDZ", .. })
    ));
    assert!(usdz_package(&[]).is_ok(), "an empty archive is still a zip");
}

#[test]
fn writing_a_file_and_reporting_errors() {
    let stage = stage();
    let dir = std::env::temp_dir().join(format!("limner-usdz-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stage.usdz");
    let report = write_usdz(
        &stage.doc,
        &path,
        &ExportOptions::still(0),
        &UsdOptions::default(),
    )
    .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(report.bytes, bytes.len() as u64);
    read_usdz(&bytes);
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(matches!(
        write_usdz(
            &stage.doc,
            &dir.join("nowhere").join("stage.usdz"),
            &ExportOptions::still(0),
            &UsdOptions::default()
        ),
        Err(ExportError::Io { .. })
    ));
    assert!(matches!(
        export_usdz(
            &stage.doc,
            &ExportOptions::animated(10 * FRAME, 0, RATE),
            &UsdOptions::default()
        ),
        Err(ExportError::InvalidRange { .. })
    ));
}

/// Runs Apple's `usdchecker` and `usdcat` on a package when they are installed.
/// Ignored by default because it depends on the machine:
///
/// ```text
/// cargo test -p limner-export --test usdz -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs Apple's USD tools in /usr/bin"]
fn apple_usd_tools_accept_the_package() {
    let checker = std::path::Path::new("/usr/bin/usdchecker");
    let cat = std::path::Path::new("/usr/bin/usdcat");
    if !checker.is_file() || !cat.is_file() {
        eprintln!("skipped: Apple's USD tools are not on this machine");
        return;
    }
    let stage = stage();
    let dir = std::env::temp_dir().join(format!("limner-usdz-tools-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stage.usdz");
    let options = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    write_usdz(&stage.doc, &path, &options, &UsdOptions::default()).unwrap();

    let output = std::process::Command::new(checker)
        .arg(&path)
        .output()
        .expect("usdchecker runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("usdchecker: {}", stdout.trim());
    assert!(
        output.status.success() && stdout.contains("Success"),
        "{stdout}"
    );

    let output = std::process::Command::new(cat)
        .arg("--loadOnly")
        .arg(&path)
        .output()
        .expect("usdcat runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("usdcat --loadOnly: {}", stdout.trim());
    assert!(output.status.success() && stdout.contains("OK"), "{stdout}");
    std::fs::remove_dir_all(&dir).unwrap();
}
