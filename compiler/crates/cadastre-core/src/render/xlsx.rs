//! Excel workbooks (`.xlsx`) of the table views: a sheet named after the view, the header in
//! bold and frozen, an automatic filter, columns as wide as their content, the URLs as links.
//!
//! An `.xlsx` file is a zip archive of XML parts (Office Open XML, SpreadsheetML); the archive is
//! written here, its entries stored without compression, so that the compiler needs no
//! dependency, natively and in WebAssembly.

use std::fmt::Write;

use super::Scene;
use super::table::{Grid, grid, urls};

/// The workbook of a table view.
pub fn xlsx(scene: &Scene) -> Vec<u8> {
    let Grid { names, numeric, rows } = grid(scene);
    let sheet = sheet_name(&scene.graph.view);
    let columns = names.len().max(1);
    let last = format!("{}{}", column_letters(columns - 1), rows.len() + 1);

    let mut data = String::new();
    let mut links: Vec<(String, String)> = Vec::new();
    data.push_str("<row r=\"1\">");
    for (c, name) in names.iter().enumerate() {
        let _ =
            write!(data, "<c r=\"{}1\" t=\"inlineStr\" s=\"1\"><is><t>{}</t></is></c>", column_letters(c), xml(name));
    }
    data.push_str("</row>");
    for (r, row) in rows.iter().enumerate() {
        let line = r + 2;
        let _ = write!(data, "<row r=\"{line}\">");
        for (c, value) in row.iter().enumerate() {
            if value.is_empty() {
                continue;
            }
            let cell = format!("{}{line}", column_letters(c));
            if numeric[c] && value.parse::<f64>().is_ok_and(f64::is_finite) {
                let _ = write!(data, "<c r=\"{cell}\"><v>{value}</v></c>");
            } else if let Some(url) = urls(value).and_then(|u| u.first().map(|u| (*u).to_owned())) {
                let _ = write!(data, "<c r=\"{cell}\" t=\"inlineStr\" s=\"2\"><is><t>{}</t></is></c>", xml(value));
                links.push((cell, url));
            } else {
                let _ = write!(
                    data,
                    "<c r=\"{cell}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml(value)
                );
            }
        }
        data.push_str("</row>");
    }

    // Widths: the longest text of the column, within bounds.
    let mut cols = String::new();
    for (c, name) in names.iter().enumerate() {
        let longest = rows.iter().map(|row| row[c].chars().count()).chain([name.chars().count()]).max().unwrap_or(0);
        let width = (longest + 2).clamp(8, 60);
        let _ = write!(cols, "<col min=\"{0}\" max=\"{0}\" width=\"{width}\" customWidth=\"1\"/>", c + 1);
    }

    let mut worksheet = format!(
        "{HEADER}<worksheet xmlns=\"{MAIN}\" xmlns:r=\"{RELATIONSHIPS}\">\
         <sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews>\
         <sheetFormatPr defaultRowHeight=\"15\"/><cols>{cols}</cols><sheetData>{data}</sheetData><autoFilter ref=\"A1:{last}\"/>"
    );
    let mut sheet_rels = None;
    if !links.is_empty() {
        worksheet.push_str("<hyperlinks>");
        let mut rels = format!("{HEADER}<Relationships xmlns=\"{PACKAGE_RELATIONSHIPS}\">");
        for (i, (cell, url)) in links.iter().enumerate() {
            let _ = write!(worksheet, "<hyperlink ref=\"{cell}\" r:id=\"rId{}\"/>", i + 1);
            let _ = write!(
                rels,
                "<Relationship Id=\"rId{}\" Type=\"{HYPERLINK}\" Target=\"{}\" TargetMode=\"External\"/>",
                i + 1,
                xml(url)
            );
        }
        worksheet.push_str("</hyperlinks>");
        rels.push_str("</Relationships>");
        sheet_rels = Some(rels);
    }
    worksheet.push_str("</worksheet>");

    let workbook = format!(
        "{HEADER}<workbook xmlns=\"{MAIN}\" xmlns:r=\"{RELATIONSHIPS}\"><sheets><sheet name=\"{0}\" sheetId=\"1\" r:id=\"rId1\"/></sheets>\
         <definedNames><definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"0\" hidden=\"1\">'{1}'!$A$1:${2}${3}</definedName></definedNames></workbook>",
        xml(&sheet),
        xml(&sheet.replace('\'', "''")),
        column_letters(columns - 1),
        rows.len() + 1
    );

    let mut parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELS.to_owned()),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.to_owned()),
        ("xl/styles.xml", STYLES.to_owned()),
        ("xl/worksheets/sheet1.xml", worksheet),
    ];
    if let Some(rels) = sheet_rels {
        parts.push(("xl/worksheets/_rels/sheet1.xml.rels", rels));
    }
    zip(&parts)
}

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\
</Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
</Relationships>";

const WORKBOOK_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
</Relationships>";

/// Styles: 0 the default, 1 the header (bold), 2 a link (blue, underlined).
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
<fonts count=\"3\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font>\
<font><u/><sz val=\"11\"/><color rgb=\"FF0563C1\"/><name val=\"Calibri\"/></font></fonts>\
<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
<cellXfs count=\"3\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>\
<xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs>\
<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
</styleSheet>";

/// A sheet name: at most 31 characters, without `: \ / ? * [ ]`.
fn sheet_name(view: &str) -> String {
    let name: String =
        view.chars().filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')).take(31).collect();
    if name.trim().is_empty() { "view".to_owned() } else { name }
}

/// `A`, …, `Z`, `AA`, …: the letters of a column, from 0.
fn column_letters(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().map(|&b| b as char).collect()
}

/// Text in XML: its special characters escaped, the control characters XML forbids dropped.
fn xml(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Zip archive, entries stored without compression
// ---------------------------------------------------------------------------

fn zip(parts: &[(&str, String)]) -> Vec<u8> {
    // 1980-01-01 00:00, the first date of the format: a stable archive for a stable content.
    const DATE: u16 = (1 << 5) | 1;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content) in parts {
        let data = content.as_bytes();
        let crc = crc32(data);
        let offset = out.len() as u32;
        let size = data.len() as u32;
        // Local file header.
        out.extend(0x0403_4b50_u32.to_le_bytes());
        out.extend(20_u16.to_le_bytes()); // version needed
        out.extend(0_u16.to_le_bytes()); // flags
        out.extend(0_u16.to_le_bytes()); // method: stored
        out.extend(0_u16.to_le_bytes()); // time
        out.extend(DATE.to_le_bytes());
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0_u16.to_le_bytes()); // extra field
        out.extend(name.as_bytes());
        out.extend(data);
        // Central directory entry.
        central.extend(0x0201_4b50_u32.to_le_bytes());
        central.extend(20_u16.to_le_bytes()); // version made by
        central.extend(20_u16.to_le_bytes()); // version needed
        central.extend(0_u16.to_le_bytes());
        central.extend(0_u16.to_le_bytes());
        central.extend(0_u16.to_le_bytes());
        central.extend(DATE.to_le_bytes());
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0_u8; 12]); // extra, comment, disk, internal and external attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let start = out.len() as u32;
    let length = central.len() as u32;
    out.extend(central);
    // End of central directory.
    out.extend(0x0605_4b50_u32.to_le_bytes());
    out.extend([0_u8; 4]); // disk numbers
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend(length.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0_u16.to_le_bytes()); // comment
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_of_known_texts() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn columns_and_sheet_names() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
        assert_eq!(sheet_name("flows/[by site]"), "flowsby site");
        assert_eq!(sheet_name(&"x".repeat(40)).len(), 31);
    }
}
