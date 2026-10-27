//! The `schedule` of a job: a cron expression, checked and put in plain words, or free text.
//!
//! A cron expression is recognized by its shape: five fields (minute, hour, day of the month,
//! month, day of the week) of numbers, `*`, `,`, `-`, `/` and names (`JAN`, `MON`), or a macro
//! (`@daily`). Anything else is free text ("10h, 12h10, 19h"), kept as written.

/// The schedule of a resource in plain words, with its time zone (its own, otherwise its
/// scheduler's): `every day at 03:00 (Europe/Paris)`; `None` without a cron expression.
pub fn plain(
    registry: &crate::registry::Registry,
    model: &crate::check::Model,
    id: crate::registry::ResourceId,
) -> Option<String> {
    let resource = registry.resource(id);
    let text = crate::query::value_text(&resource.attribute("schedule")?.value);
    let Ok(Schedule::Cron(cron)) = parse(&text) else { return None };
    let timezone = resource.attribute("timezone").or_else(|| {
        let scheduler = model.reference(crate::check::Holder::Resource(id), "scheduler").first()?;
        registry.resource(*scheduler).attribute("timezone")
    });
    Some(match timezone {
        Some(zone) => format!("{} ({})", cron.describe(), crate::query::value_text(&zone.value)),
        None => cron.describe(),
    })
}

/// A schedule as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Schedule {
    Cron(Box<Cron>),
    Text(String),
}

/// The five fields of a cron expression, each a set of values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cron {
    pub expression: String,
    fields: [Field; 5],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    /// As written, normalized (names in upper case).
    text: String,
    /// The values the field matches, sorted.
    values: Vec<u32>,
}

/// Name, bounds and value names of each field.
const FIELDS: [(&str, u32, u32, &[&str]); 5] = [
    ("minute", 0, 59, &[]),
    ("hour", 0, 23, &[]),
    ("day of the month", 1, 31, &[]),
    ("month", 1, 12, &["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]),
    ("day of the week", 0, 7, &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]),
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Reads a schedule: a cron expression when it has the shape of one (then checked), free text
/// otherwise. The shape of one: four to six fields, all of them but one at most made of cron
/// values (digits, `*`, `,`, `-`, `/`, names of months and days): `0 3 * FOO *` is a wrong cron
/// expression, `toutes les 15 min` free text. The error says what is wrong in a cron expression.
pub fn parse(text: &str) -> Result<Schedule, String> {
    let trimmed = text.trim();
    let expanded = match trimmed.to_ascii_lowercase().as_str() {
        "@yearly" | "@annually" => Some("0 0 1 1 *"),
        "@monthly" => Some("0 0 1 * *"),
        "@weekly" => Some("0 0 * * 0"),
        "@daily" | "@midnight" => Some("0 0 * * *"),
        "@hourly" => Some("0 * * * *"),
        macro_ if macro_.starts_with('@') => {
            return Err(format!(
                "unknown cron macro: {trimmed} (@yearly, @annually, @monthly, @weekly, @daily, @midnight, @hourly)"
            ));
        }
        _ => None,
    };
    let source = expanded.unwrap_or(trimmed);
    let parts: Vec<&str> = source.split_whitespace().collect();
    let names = || FIELDS.iter().flat_map(|f| f.3.iter());
    let cron_value = |p: &&str| {
        p.to_ascii_uppercase()
            .split([',', '-', '/'])
            .all(|v| !v.is_empty() && (v == "*" || v.chars().all(|c| c.is_ascii_digit()) || names().any(|n| *n == v)))
    };
    let shaped = (4..=6).contains(&parts.len()) && parts.iter().filter(|p| cron_value(p)).count() + 1 >= parts.len();
    if expanded.is_none() && !shaped {
        return Ok(Schedule::Text(trimmed.to_owned()));
    }
    if parts.len() != 5 {
        return Err(format!(
            "a cron expression has five fields (minute hour day-of-month month day-of-week), not {}",
            parts.len()
        ));
    }
    let mut fields = Vec::new();
    for (part, &(name, low, high, names)) in parts.iter().zip(&FIELDS) {
        fields.push(field(part, name, low, high, names)?);
    }
    let fields: [Field; 5] = fields.try_into().expect("five fields");
    Ok(Schedule::Cron(Box::new(Cron { expression: trimmed.to_owned(), fields })))
}

fn field(text: &str, name: &str, low: u32, high: u32, names: &[&str]) -> Result<Field, String> {
    let upper = text.to_ascii_uppercase();
    let value = |v: &str| -> Result<u32, String> {
        if let Some(i) = names.iter().position(|n| *n == v) {
            return Ok(i as u32 + if low == 1 { 1 } else { 0 });
        }
        let n: u32 = v.parse().map_err(|_| format!("{name}: `{v}` is not a value"))?;
        if n < low || n > high {
            return Err(format!("{name}: {n} is out of range ({low}-{high})"));
        }
        Ok(n)
    };
    let mut values = Vec::new();
    for item in upper.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((range, step)) => {
                let step: u32 =
                    step.parse().ok().filter(|&s| s > 0).ok_or(format!("{name}: `/{step}` is not a step"))?;
                (range, step)
            }
            None => (item, 1),
        };
        let (start, end) = match range {
            "*" => (low, high),
            _ => match range.split_once('-') {
                Some((a, b)) => (value(a)?, value(b)?),
                // `5/15`: from 5, every 15.
                None if step > 1 => (value(range)?, high),
                None => {
                    let v = value(range)?;
                    (v, v)
                }
            },
        };
        if start > end {
            return Err(format!("{name}: the range `{range}` is reversed"));
        }
        values.extend((start..=end).step_by(step as usize));
    }
    // Sunday is 0 and 7.
    if name == "day of the week" {
        for v in &mut values {
            if *v == 7 {
                *v = 0;
            }
        }
    }
    values.sort_unstable();
    values.dedup();
    Ok(Field { text: upper, values })
}

impl Cron {
    /// In plain words: "every day at 03:00", "every 15 minutes", "at 08:00 and 18:00 on Monday to
    /// Friday"; the expression itself when it says more than these words can.
    pub fn describe(&self) -> String {
        let [minute, hour, day, month, weekday] = &self.fields;
        let all = |f: &Field, (_, low, high, _): (&str, u32, u32, &[&str])| {
            f.text == "*" || f.values.len() as u32 == high - low + 1
        };
        let every_day =
            all(day, FIELDS[2]) && all(month, FIELDS[3]) && (weekday.text == "*" || weekday.values.len() == 7);
        let Some(time) = time(minute, hour) else { return format!("cron {}", self.expression) };
        let mut days = Vec::new();
        if !all(day, FIELDS[2]) && weekday.text != "*" {
            // Day of the month or day of the week: no short words for it.
            return format!("cron {}", self.expression);
        }
        if !all(day, FIELDS[2]) {
            let list: Vec<String> = day.values.iter().map(u32::to_string).collect();
            days.push(format!("on day{} {} of the month", if list.len() > 1 { "s" } else { "" }, and(&list)));
        }
        if weekday.text != "*" && weekday.values.len() < 7 {
            days.push(format!("on {}", weekdays(&weekday.values)));
        }
        if !all(month, FIELDS[3]) {
            let list: Vec<String> = month.values.iter().map(|&m| MONTHS[m as usize - 1].to_owned()).collect();
            days.push(format!("in {}", and(&list)));
        }
        match (time, every_day) {
            (Time::At(at), true) => format!("every day at {at}"),
            (Time::At(at), false) => format!("at {at} {}", days.join(" ")),
            (Time::Every(every), true) => every,
            (Time::Every(every), false) => format!("{every} {}", days.join(" ")),
        }
    }
}

enum Time {
    /// `03:00`, `08:00 and 18:00`.
    At(String),
    /// `every 15 minutes`, `every hour`.
    Every(String),
}

fn time(minute: &Field, hour: &Field) -> Option<Time> {
    let step =
        |f: &Field| f.text.strip_prefix("*/").or_else(|| f.text.strip_prefix("0/")).and_then(|s| s.parse::<u32>().ok());
    let clock = |h: u32, m: u32| format!("{h:02}:{m:02}");
    match (minute.text.as_str(), hour.text.as_str()) {
        ("*", "*") => Some(Time::Every("every minute".to_owned())),
        (_, "*") if step(minute).is_some() => Some(Time::Every(format!("every {} minutes", step(minute)?))),
        (_, "*") if minute.values.len() == 1 => Some(Time::Every(match minute.values[0] {
            0 => "every hour".to_owned(),
            m => format!("every hour at minute {m}"),
        })),
        _ if minute.values.len() == 1 && step(hour).is_some() => {
            let m = minute.values[0];
            let every = format!("every {} hours", step(hour)?);
            Some(Time::Every(if m == 0 { every } else { format!("{every} at minute {m}") }))
        }
        _ if minute.values.len() * hour.values.len() <= 6
            && !minute.text.contains(['*', '/'])
            && !hour.text.contains(['*', '/']) =>
        {
            let times: Vec<String> =
                hour.values.iter().flat_map(|&h| minute.values.iter().map(move |&m| clock(h, m))).collect();
            Some(Time::At(and(&times)))
        }
        _ => None,
    }
}

/// `Monday to Friday`, `Monday, Wednesday and Friday`.
fn weekdays(values: &[u32]) -> String {
    let consecutive = values.windows(2).all(|w| w[1] == w[0] + 1);
    if values.len() > 2 && consecutive {
        return format!("{} to {}", DAYS[values[0] as usize], DAYS[values[values.len() - 1] as usize]);
    }
    and(&values.iter().map(|&d| DAYS[d as usize].to_owned()).collect::<Vec<_>>())
}

/// `a`, `a and b`, `a, b and c`.
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> String {
        match parse(text).unwrap() {
            Schedule::Cron(cron) => cron.describe(),
            Schedule::Text(text) => format!("text: {text}"),
        }
    }

    #[test]
    fn plain_words() {
        assert_eq!(words("0 3 * * *"), "every day at 03:00");
        assert_eq!(words("@daily"), "every day at 00:00");
        assert_eq!(words("*/15 * * * *"), "every 15 minutes");
        assert_eq!(words("0 * * * *"), "every hour");
        assert_eq!(words("30 * * * *"), "every hour at minute 30");
        assert_eq!(words("0 */2 * * *"), "every 2 hours");
        assert_eq!(words("0 8,12,18 * * 1-5"), "at 08:00, 12:00 and 18:00 on Monday to Friday");
        assert_eq!(words("30 22 * * SAT,SUN"), "at 22:30 on Sunday and Saturday");
        assert_eq!(words("0 6 1,15 * *"), "at 06:00 on days 1 and 15 of the month");
        assert_eq!(words("0 0 1 JAN *"), "at 00:00 on day 1 of the month in January");
        assert_eq!(words("0 7 * * 7"), "at 07:00 on Sunday");
        assert_eq!(words("5,10,15,20,25,30,35 * * * *"), "cron 5,10,15,20,25,30,35 * * * *");
    }

    #[test]
    fn free_text() {
        assert_eq!(words("10h, 12h10, 19h"), "text: 10h, 12h10, 19h");
        assert_eq!(words("cron"), "text: cron");
        assert_eq!(words("daily at 2am"), "text: daily at 2am");
        assert_eq!(words("toutes les 15 min"), "text: toutes les 15 min");
        assert_eq!(words("every 30 min on weekdays"), "text: every 30 min on weekdays");
        assert_eq!(words("lundi au vendredi à 3h"), "text: lundi au vendredi à 3h");
    }

    #[test]
    fn errors() {
        assert_eq!(parse("0 25 * * *").unwrap_err(), "hour: 25 is out of range (0-23)");
        assert_eq!(
            parse("0 3 * *").unwrap_err(),
            "a cron expression has five fields (minute hour day-of-month month day-of-week), not 4"
        );
        assert_eq!(parse("0 3 * FOO *").unwrap_err(), "month: `FOO` is not a value");
        assert_eq!(parse("0 18-8 * * *").unwrap_err(), "hour: the range `18-8` is reversed");
        assert!(parse("@often").unwrap_err().starts_with("unknown cron macro"));
    }
}
