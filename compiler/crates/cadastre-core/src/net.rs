//! IP addresses, CIDRs, host names and URLs, for consistency checks (spec § 5).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ip {
    V4(u32),
    V6(u128),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    pub network: Ip,
    pub prefix: u8,
}

/// A DNS host name (RFC 1123): dot-separated labels of letters, digits and hyphens, not starting
/// nor ending with a hyphen, 63 characters at most each, 253 in total; returned in lowercase.
pub fn host_name(text: &str) -> Option<String> {
    let valid = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    (text.len() <= 253 && text.split('.').all(valid)).then(|| text.to_ascii_lowercase())
}

/// Why a text is not an accepted URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UrlError {
    Malformed,
    /// A password in the URL (`user:secret@host`).
    Password,
    /// A query parameter holding a secret (`?token=…`).
    Secret(String),
}

/// Query parameters taken for secrets.
const SECRET_PARAMETERS: &[&str] =
    &["password", "passwd", "pwd", "secret", "token", "access_token", "apikey", "api_key", "api-key"];

/// Checks a URL: `scheme://[user@]host[:port][/path][?query][#fragment]`, the host being a host
/// name, an IPv4 address or a bracketed IPv6 address. Neither a password nor a secret query
/// parameter is accepted: the registry is versioned and read by everyone.
pub fn check_url(text: &str) -> Result<(), UrlError> {
    let (scheme, rest) = text.split_once("://").ok_or(UrlError::Malformed)?;
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    if !scheme_ok || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UrlError::Malformed);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host_port = match authority.rsplit_once('@') {
        Some((user, host_port)) => {
            if user.contains(':') {
                return Err(UrlError::Password);
            }
            if user.is_empty() {
                return Err(UrlError::Malformed);
            }
            host_port
        }
        None => authority,
    };
    let (host, port) = if let Some(inner) = host_port.strip_prefix('[') {
        let (address, after) = inner.split_once(']').ok_or(UrlError::Malformed)?;
        parse_v6(address).ok_or(UrlError::Malformed)?;
        match after {
            "" => (address, None),
            _ => (address, Some(after.strip_prefix(':').ok_or(UrlError::Malformed)?)),
        }
    } else {
        match host_port.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        }
    };
    if host.is_empty() || (parse_ip(host).is_none() && host_name(host).is_none()) {
        return Err(UrlError::Malformed);
    }
    if let Some(port) = port
        && !matches!(port.parse::<u16>(), Ok(1..))
    {
        return Err(UrlError::Malformed);
    }
    if let Some((_, query)) = tail.split_once('?') {
        let query = query.split('#').next().unwrap_or_default();
        for parameter in query.split('&') {
            let name = parameter.split('=').next().unwrap_or_default().to_ascii_lowercase();
            if SECRET_PARAMETERS.contains(&name.as_str()) {
                return Err(UrlError::Secret(name));
            }
        }
    }
    Ok(())
}

pub fn parse_ip(text: &str) -> Option<Ip> {
    if text.contains(':') { parse_v6(text).map(Ip::V6) } else { parse_v4(text).map(Ip::V4) }
}

pub fn parse_cidr(text: &str) -> Option<Cidr> {
    let (ip, prefix) = text.split_once('/')?;
    let network = parse_ip(ip)?;
    let prefix: u8 = prefix.parse().ok()?;
    let max = match network {
        Ip::V4(_) => 32,
        Ip::V6(_) => 128,
    };
    (prefix <= max).then_some(Cidr { network, prefix })
}

impl Cidr {
    /// Whether two CIDRs share addresses: one contains the other.
    pub fn overlaps(&self, other: &Cidr) -> bool {
        self.contains(other.network) || other.contains(self.network)
    }

    pub fn contains(&self, ip: Ip) -> bool {
        match (self.network, ip) {
            (Ip::V4(net), Ip::V4(addr)) => {
                let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - self.prefix) };
                net & mask == addr & mask
            }
            (Ip::V6(net), Ip::V6(addr)) => {
                let mask = if self.prefix == 0 { 0 } else { u128::MAX << (128 - self.prefix) };
                net & mask == addr & mask
            }
            _ => false,
        }
    }
}

fn parse_v4(text: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut count = 0;
    for part in text.split('.') {
        let octet: u8 = part.parse().ok()?;
        value = value << 8 | u32::from(octet);
        count += 1;
    }
    (count == 4).then_some(value)
}

fn parse_v6(text: &str) -> Option<u128> {
    let groups = |s: &str| -> Option<Vec<u16>> {
        if s.is_empty() {
            return Some(Vec::new());
        }
        s.split(':').map(|g| u16::from_str_radix(g, 16).ok()).collect()
    };
    let (head, tail) = match text.split_once("::") {
        Some((h, t)) => (groups(h)?, Some(groups(t)?)),
        None => (groups(text)?, None),
    };
    let all: Vec<u16> = match tail {
        Some(tail) => {
            let missing = 8usize.checked_sub(head.len() + tail.len())?;
            head.iter().copied().chain(std::iter::repeat_n(0, missing)).chain(tail).collect()
        }
        None => head,
    };
    (all.len() == 8).then(|| all.iter().fold(0u128, |acc, &g| acc << 16 | u128::from(g)))
}

/// A bandwidth in bits per second: a number and a decimal unit, `bps`, `kbps`, `Mbps`, `Gbps`,
/// `Tbps` or `bit/s`, `kbit/s`…, with or without a space (`100Mbps`, `"1.5 Gbit/s"`).
pub fn bandwidth(text: &str) -> Option<f64> {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let number: f64 = text[..split].parse().ok()?;
    let unit = text[split..].trim_start().to_ascii_lowercase();
    let unit = unit.strip_suffix("bps").or_else(|| unit.strip_suffix("bit/s"))?;
    let factor = match unit {
        "" => 1e0,
        "k" => 1e3,
        "m" => 1e6,
        "g" => 1e9,
        "t" => 1e12,
        _ => return None,
    };
    Some(number * factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        for ok in [
            "https://labels.example.com",
            "http://sales.example.com:8080/catalogue/web?lang=fr#top",
            "ssh://admin@fw01.example.com",
            "rdp://10.0.1.12:3389",
            "https://[fd00::1]:8443/",
            "vnc://kiosk-03",
        ] {
            assert_eq!(check_url(ok), Ok(()), "{ok}");
        }
        for malformed in [
            "labels.example.com",
            "https://",
            "https://a b.example.com",
            "https://host:0",
            "https://host:99999",
            "https://-host",
            "1http://host",
            "https://@host",
            "https://[fd00::1",
        ] {
            assert_eq!(check_url(malformed), Err(UrlError::Malformed), "{malformed}");
        }
        assert_eq!(check_url("https://admin:hunter2@host"), Err(UrlError::Password));
        assert_eq!(check_url("https://host/api?Token=abc"), Err(UrlError::Secret("token".into())));
    }

    #[test]
    fn ipv4_membership() {
        let net = parse_cidr("10.0.1.0/24").unwrap();
        assert!(net.contains(parse_ip("10.0.1.10").unwrap()));
        assert!(!net.contains(parse_ip("10.0.2.1").unwrap()));
    }

    #[test]
    fn ipv6_membership() {
        let net = parse_cidr("fd00::/8").unwrap();
        assert!(net.contains(parse_ip("fd00::1").unwrap()));
        assert!(!net.contains(parse_ip("2001:db8::1").unwrap()));
        assert_eq!(parse_ip("::1"), Some(Ip::V6(1)));
        assert_eq!(parse_ip("2001:db8:0:0:0:0:0:1"), parse_ip("2001:db8::1"));
    }
}
