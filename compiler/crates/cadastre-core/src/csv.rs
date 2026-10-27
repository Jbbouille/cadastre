//! CSV files read by `for_each … in csv("stores.csv")`: UTF-8 (a byte order mark allowed), the
//! separator `,` or `;` (as Excel writes it in many languages) found on the header line, fields
//! quoted as RFC 4180 says, CRLF or LF line endings.

/// A field, as its text and its byte range in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// The records of a file, the header first; empty lines are skipped. An error is a byte offset
/// and what is wrong there.
pub fn parse(text: &str) -> Result<Vec<Vec<Field>>, (usize, String)> {
    let start = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let separator = separator(&text[start..]);
    let bytes = text.as_bytes();
    let mut records = Vec::new();
    let mut record: Vec<Field> = Vec::new();
    let mut at = start;
    loop {
        // A field.
        let field_start = at;
        let mut value = String::new();
        if bytes.get(at) == Some(&b'"') {
            at += 1;
            loop {
                match text[at..].find('"') {
                    None => return Err((field_start, "a quoted field without its closing quote".to_owned())),
                    Some(quote) => {
                        value.push_str(&text[at..at + quote]);
                        at += quote + 1;
                        if bytes.get(at) == Some(&b'"') {
                            value.push('"');
                            at += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
            if !matches!(bytes.get(at), None | Some(b'\r' | b'\n')) && bytes.get(at) != Some(&(separator as u8)) {
                return Err((at, format!("`{separator}` or the end of the line expected after a quoted field")));
            }
        } else {
            let end = text[at..].find([separator, '\r', '\n']).map_or(text.len(), |e| at + e);
            value.push_str(&text[at..end]);
            at = end;
        }
        record.push(Field { text: value, start: field_start, end: at });
        // What follows it: another field, the end of the record, or of the file.
        match bytes.get(at) {
            Some(&c) if c == separator as u8 => at += 1,
            Some(b'\r' | b'\n') | None => {
                let blank = record.len() == 1 && record[0].text.is_empty() && record[0].start == record[0].end;
                if !blank {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
                if bytes.get(at) == Some(&b'\r') {
                    at += 1;
                }
                if bytes.get(at) == Some(&b'\n') {
                    at += 1;
                }
                if at >= bytes.len() {
                    return Ok(records);
                }
            }
            Some(_) => unreachable!(),
        }
    }
}

/// `;` when the header line has more of them than of commas, outside quotes.
fn separator(text: &str) -> char {
    let header = text.lines().next().unwrap_or_default();
    let (mut commas, mut semicolons, mut quoted) = (0, 0, false);
    for c in header.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => commas += 1,
            ';' if !quoted => semicolons += 1,
            _ => {}
        }
    }
    if semicolons > commas { ';' } else { ',' }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<Vec<String>> {
        parse(text).unwrap().into_iter().map(|r| r.into_iter().map(|f| f.text).collect()).collect()
    }

    #[test]
    fn separators_quotes_and_lines() {
        assert_eq!(
            texts("code,ip\nlil,10.1.1.10\r\nlyo,10.1.2.10\n\n"),
            [["code", "ip"], ["lil", "10.1.1.10"], ["lyo", "10.1.2.10"]]
        );
        assert_eq!(texts("\u{feff}code;label\nlil;\"Lille; Nord\"\n"), [["code", "label"], ["lil", "Lille; Nord"]]);
        assert_eq!(texts("a,b\n\"say \"\"hi\"\"\",\n"), [["a", "b"], ["say \"hi\"", ""]]);
        assert_eq!(texts("a,b\n\"two\nlines\",x"), [["a", "b"], ["two\nlines", "x"]]);
        assert!(parse("a\n\"open").is_err());
    }

    #[test]
    fn offsets() {
        let records = parse("code,ip\nlil,10.1.1.10\n").unwrap();
        let ip = &records[1][1];
        assert_eq!((ip.start, ip.end), (12, 21));
    }
}
