//! Excel workbooks (`.xlsx`) of the table views: a sheet per view, named after it, the header in
//! bold and frozen, an automatic filter, columns as wide as their content, the URLs as links.
//!
//! An `.xlsx` file is a zip archive of XML parts (Office Open XML, SpreadsheetML); the archive is
//! written here, its entries stored without compression, so that the compiler needs no
//! dependency, natively and in WebAssembly.

use std::fmt::Write;

use std::collections::HashMap;

use super::Scene;
use super::table::{Grid, grid, urls};
use crate::registry::ResourceId;

/// The workbook of a table view.
pub fn xlsx(scene: &Scene) -> Vec<u8> {
    workbook(&[scene])
}

/// A workbook of several table views, a sheet per view in their order (`sheets`).
pub fn workbook(scenes: &[&Scene]) -> Vec<u8> {
    let mut names: Vec<String> = Vec::new();
    for scene in scenes {
        let base = sheet_name(&scene.graph.view);
        // Names are unique in a workbook, whatever their case; cut at 31 characters, two views
        // could give the same.
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|other| other.to_lowercase() == name.to_lowercase()) {
            let suffix = format!(" ({n})");
            name = format!("{}{suffix}", base.chars().take(31 - suffix.len()).collect::<String>());
            n += 1;
        }
        names.push(name);
    }
    // The rows of each resource, in each sheet: where a cell naming it leads.
    let grids: Vec<Grid> = scenes.iter().map(|scene| grid(scene)).collect();
    let mut rows_of: HashMap<ResourceId, Vec<(usize, usize)>> = HashMap::new();
    for (s, grid) in grids.iter().enumerate() {
        for (r, subject) in grid.subjects.iter().enumerate() {
            if let Some(id) = subject {
                let rows = rows_of.entry(*id).or_default();
                if !rows.iter().any(|&(other, _)| other == s) {
                    rows.push((s, r + 2));
                }
            }
        }
    }
    let sheets: Vec<Sheet> = grids.into_iter().enumerate().map(|(s, grid)| sheet(s, grid, &names, &rows_of)).collect();

    let mut list = String::new();
    let mut filters = String::new();
    let mut types = String::new();
    let mut relationships = String::new();
    for (i, (name, sheet)) in names.iter().zip(&sheets).enumerate() {
        let n = i + 1;
        let _ = write!(list, "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>", xml(name));
        let _ = write!(
            filters,
            "<definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"{i}\" hidden=\"1\">'{}'!$A$1:${}${}</definedName>",
            xml(&name.replace('\'', "''")),
            column_letters(sheet.columns - 1),
            sheet.rows + 1
        );
        let _ = write!(
            types,
            "<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        );
        let _ = write!(
            relationships,
            "<Relationship Id=\"rId{n}\" Type=\"{WORKSHEET}\" Target=\"worksheets/sheet{n}.xml\"/>"
        );
    }
    let workbook = format!(
        "{HEADER}<workbook xmlns=\"{MAIN}\" xmlns:r=\"{RELATIONSHIPS}\"><sheets>{list}</sheets>\
         <definedNames>{filters}</definedNames></workbook>"
    );
    let content_types = format!(
        "{HEADER}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
         {types}\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\
         </Types>"
    );
    let workbook_rels = format!(
        "{HEADER}<Relationships xmlns=\"{PACKAGE_RELATIONSHIPS}\">{relationships}\
         <Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
         </Relationships>",
        sheets.len() + 1
    );

    let mut parts: Vec<(String, String)> = vec![
        ("[Content_Types].xml".to_owned(), content_types),
        ("_rels/.rels".to_owned(), ROOT_RELS.to_owned()),
        ("xl/workbook.xml".to_owned(), workbook),
        ("xl/_rels/workbook.xml.rels".to_owned(), workbook_rels),
        ("xl/styles.xml".to_owned(), STYLES.to_owned()),
    ];
    for (i, sheet) in sheets.into_iter().enumerate() {
        parts.push((format!("xl/worksheets/sheet{}.xml", i + 1), sheet.worksheet));
        if let Some(rels) = sheet.rels {
            parts.push((format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1), rels));
        }
    }
    zip(&parts)
}

/// The worksheet of a view, its links, and its size for the filter of the workbook.
struct Sheet {
    worksheet: String,
    rels: Option<String>,
    columns: usize,
    rows: usize,
}

/// The sheet `index` of a workbook: a cell naming a resource listed by a sheet links to its row,
/// in the same sheet if it is there, otherwise in the first sheet listing it.
fn sheet(index: usize, grid: Grid, sheets: &[String], rows_of: &HashMap<ResourceId, Vec<(usize, usize)>>) -> Sheet {
    let Grid { names, numeric, rows, resources, .. } = grid;
    let columns = names.len().max(1);
    let last = format!("{}{}", column_letters(columns - 1), rows.len() + 1);

    let mut data = String::new();
    let mut links: Vec<(String, String)> = Vec::new();
    // Links within the workbook: the cell, the cell it leads to (`'servers'!A5`).
    let mut inside: Vec<(String, String)> = Vec::new();
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
            } else if let Some(&(s, target)) = resources[r][c].and_then(|id| rows_of.get(&id)).and_then(|rows| {
                rows.iter()
                    .find(|&&(s, row)| s == index && row != line)
                    .or_else(|| rows.iter().find(|&&(s, _)| s != index))
            }) {
                let _ = write!(data, "<c r=\"{cell}\" t=\"inlineStr\" s=\"2\"><is><t>{}</t></is></c>", xml(value));
                inside.push((cell, format!("'{}'!A{target}", sheets[s].replace('\'', "''"))));
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
    let mut rels = None;
    if !links.is_empty() || !inside.is_empty() {
        worksheet.push_str("<hyperlinks>");
        for (cell, location) in &inside {
            let _ = write!(worksheet, "<hyperlink ref=\"{cell}\" location=\"{}\"/>", xml(location));
        }
        let mut targets = format!("{HEADER}<Relationships xmlns=\"{PACKAGE_RELATIONSHIPS}\">");
        for (i, (cell, url)) in links.iter().enumerate() {
            let _ = write!(worksheet, "<hyperlink ref=\"{cell}\" r:id=\"rId{}\"/>", i + 1);
            let _ = write!(
                targets,
                "<Relationship Id=\"rId{}\" Type=\"{HYPERLINK}\" Target=\"{}\" TargetMode=\"External\"/>",
                i + 1,
                xml(url)
            );
        }
        worksheet.push_str("</hyperlinks>");
        targets.push_str("</Relationships>");
        rels = (!links.is_empty()).then_some(targets);
    }
    worksheet.push_str("</worksheet>");
    Sheet { worksheet, rels, columns, rows: rows.len() }
}

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const WORKSHEET: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
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

fn zip(parts: &[(String, String)]) -> Vec<u8> {
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
