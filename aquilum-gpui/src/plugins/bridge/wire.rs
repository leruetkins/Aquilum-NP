/// Validates that an outgoing HTTP request URL is secure against SSRF and matches the plugin's domain whitelist.
pub fn validate_network_url(url_str: &str, whitelist: &[String]) -> Result<(), String> {
    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
        return Err("Only http:// and https:// URL schemes are permitted".to_string());
    }

    let url_without_scheme = if let Some(rest) = url_str.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url_str.strip_prefix("http://") {
        rest
    } else {
        return Err("Invalid URL scheme".to_string());
    };

    let host = url_without_scheme
        .split(&['/', '?', '#', ':'][..])
        .next()
        .unwrap_or("")
        .to_lowercase();

    if host.is_empty() {
        return Err("URL host cannot be empty".to_string());
    }

    // Reject localhost, link-local, loopback and broadcast
    if host == "localhost"
        || host == "127.0.0.1"
        || host == "0.0.0.0"
        || host == "::1"
        || host == "[::1]"
        || host.starts_with("127.")
        || host.starts_with("169.254.")
    {
        return Err("Access to localhost and loopback interfaces is prohibited for security".to_string());
    }

    // Reject RFC 1918 private IPv4 ranges (10.0.0.0/8, 192.168.0.0/16, 172.16.0.0/12)
    if host.starts_with("10.") || host.starts_with("192.168.") {
        return Err("Access to private local network ranges is prohibited".to_string());
    }
    if let Some(second_octet) = host.strip_prefix("172.") {
        if let Some(octet_str) = second_octet.split('.').next() {
            if let Ok(octet) = octet_str.parse::<u8>() {
                if (16..=31).contains(&octet) {
                    return Err("Access to private local network ranges is prohibited".to_string());
                }
            }
        }
    }

    // Check whitelist matching
    let allowed = whitelist.iter().any(|allowed_domain| {
        if allowed_domain == "*" {
            return true;
        }
        let clean = allowed_domain.to_lowercase();
        host == clean || host.ends_with(&format!(".{}", clean))
    });

    if !allowed {
        return Err(format!("Domain '{}' is not in the plugin network whitelist", host));
    }

    Ok(())
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding, for the plugin binary transport.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(BASE64_ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64_ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64_ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Decodes standard base64 (padding optional); None on invalid input.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut value_of = [255u8; 256];
    for (i, &c) in BASE64_ALPHABET.iter().enumerate() {
        value_of[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &b in text.as_bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' || b == b' ' {
            continue;
        }
        let v = value_of[b as usize];
        if v == 255 {
            return None;
        }
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}
