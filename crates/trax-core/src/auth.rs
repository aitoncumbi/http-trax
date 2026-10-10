//! Authentication helpers: AWS Signature v4, HTTP Digest (RFC 7616), NTLM,
//! and OAuth 2.0 `private_key_jwt` client assertions (RFC 7523).

use std::collections::BTreeMap;

use base64::Engine as _;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use url::Url;

use crate::http::normalise_url;
use crate::model::{new_id, Header};
use crate::runner::set_header;

// --------------------------------------------------------------- primitives

pub fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = sha256_bytes(key);
        k[..32].copy_from_slice(&hashed);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// RFC 1321 MD5 (used for HTTP Digest MD5 challenges and NTLMv2 HMAC-MD5).
pub fn md5_bytes(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            let o = i * 4;
            *word = u32::from_le_bytes([chunk[o], chunk[o + 1], chunk[o + 2], chunk[o + 3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(m[g])
                    .rotate_left(S[i]),
            );
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

pub fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; 16] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = md5_bytes(key);
        k[..16].copy_from_slice(&hashed);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    ipad.extend_from_slice(data);
    let inner = md5_bytes(&ipad);
    opad.extend_from_slice(&inner);
    md5_bytes(&opad)
}

/// RFC 1320 MD4 (used for NTLM password hashes).
pub fn md4_bytes(input: &[u8]) -> [u8; 16] {
    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in msg.chunks_exact(64) {
        let mut x = [0u32; 16];
        for (i, word) in x.iter_mut().enumerate() {
            let o = i * 4;
            *word = u32::from_le_bytes([chunk[o], chunk[o + 1], chunk[o + 2], chunk[o + 3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);

        // Round 1
        let s1 = [3, 7, 11, 19];
        for i in 0..16 {
            let f = (b & c) | (!b & d);
            let t = a.wrapping_add(f).wrapping_add(x[i]).rotate_left(s1[i % 4]);
            (a, d, c, b) = (d, c, b, t);
        }
        // Round 2
        let idx2 = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15];
        let s2 = [3, 5, 9, 13];
        for i in 0..16 {
            let f = (b & c) | (b & d) | (c & d);
            let t = a
                .wrapping_add(f)
                .wrapping_add(x[idx2[i]])
                .wrapping_add(0x5a827999)
                .rotate_left(s2[i % 4]);
            (a, d, c, b) = (d, c, b, t);
        }
        // Round 3
        let idx3 = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let s3 = [3, 9, 11, 15];
        for i in 0..16 {
            let f = b ^ c ^ d;
            let t = a
                .wrapping_add(f)
                .wrapping_add(x[idx3[i]])
                .wrapping_add(0x6ed9eba1)
                .rotate_left(s3[i % 4]);
            (a, d, c, b) = (d, c, b, t);
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

// --------------------------------------------------------------- AWS SigV4

fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if unreserved || (*b == b'/' && !encode_slash) {
            out.push(*b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "%{b:02X}");
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn sign_aws_sigv4(
    method: &str,
    raw_url: &str,
    headers: &mut Vec<Header>,
    body: &[u8],
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
    session_token: &str,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let parsed = Url::parse(&normalise_url(raw_url))
        .map_err(|e| format!("invalid URL for AWS SigV4: {e}"))?;
    let host = match parsed.port() {
        Some(port) => format!("{}:{port}", parsed.host_str().unwrap_or_default()),
        None => parsed.host_str().unwrap_or_default().to_string(),
    };
    let region = if region.trim().is_empty() {
        "us-east-1"
    } else {
        region.trim()
    };
    let service = if service.trim().is_empty() {
        "execute-api"
    } else {
        service.trim()
    };

    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let payload_hash = hex_lower(&sha256_bytes(body));

    if !headers.iter().any(|h| h.key.eq_ignore_ascii_case("host")) {
        headers.push(Header::new("Host", &host));
    }
    set_header(headers, "x-amz-date", amz_date.clone());
    set_header(headers, "x-amz-content-sha256", payload_hash.clone());
    if !session_token.trim().is_empty() {
        set_header(
            headers,
            "x-amz-security-token",
            session_token.trim().to_string(),
        );
    }

    // Canonical URI
    let path = if parsed.path().is_empty() {
        "/"
    } else {
        parsed.path()
    };
    let canonical_uri = uri_encode(path, false);

    // Canonical query string (sorted by encoded key, then encoded value)
    let mut query_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (uri_encode(&k, true), uri_encode(&v, true)))
        .collect();
    query_pairs.sort();
    let canonical_query = query_pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    // Canonical headers (lowercased, sorted, whitespace-collapsed)
    let mut canon_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for h in headers.iter() {
        let k = h.key.trim().to_ascii_lowercase();
        if k.is_empty() || k == "authorization" {
            continue;
        }
        let collapsed = h.value.split_whitespace().collect::<Vec<_>>().join(" ");
        canon_map.entry(k).or_default().push(collapsed);
    }
    let mut canonical_headers = String::new();
    let mut signed_header_names = Vec::new();
    for (k, vals) in &canon_map {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(&vals.join(","));
        canonical_headers.push('\n');
        signed_header_names.push(k.as_str());
    }
    let signed_headers = signed_header_names.join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.trim().to_ascii_uppercase(),
        canonical_uri,
        canonical_query,
        canonical_headers,
        signed_headers,
        payload_hash
    );

    let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        hex_lower(&sha256_bytes(canonical_request.as_bytes()))
    );

    let k_secret = format!("AWS4{}", secret_key);
    let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let auth_header = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key.trim(),
        credential_scope,
        signed_headers,
        signature
    );
    set_header(headers, "Authorization", auth_header);
    Ok(())
}

// --------------------------------------------------------------- Digest Auth

/// Parse a `WWW-Authenticate: Digest ...` header into key-value pairs.
fn parse_digest_challenge(header: &str) -> Option<BTreeMap<String, String>> {
    let rest = header.trim().strip_prefix("Digest")?.trim_start();
    if !rest.is_empty() && !header.trim()[6..].starts_with(char::is_whitespace) {
        return None;
    }
    let mut params = BTreeMap::new();
    let mut chars = rest.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ',' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() != Some(&'=') {
            continue;
        }
        chars.next(); // '='
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut val = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '"' {
                    break;
                }
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        val.push(escaped);
                    }
                } else {
                    val.push(c);
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ',' || c.is_whitespace() {
                    break;
                }
                val.push(c);
                chars.next();
            }
        }
        if !key.is_empty() {
            params.insert(key.to_ascii_lowercase(), val);
        }
    }
    Some(params)
}

pub fn digest_authorization_header(
    www_authenticate: &str,
    method: &str,
    raw_url: &str,
    body: &[u8],
    username: &str,
    password: &str,
    cnonce: &str,
) -> Option<String> {
    let params = parse_digest_challenge(www_authenticate)?;
    let realm = params.get("realm")?.as_str();
    let nonce = params.get("nonce")?.as_str();
    let algorithm = params.get("algorithm").map(|s| s.as_str()).unwrap_or("MD5");
    let alg_upper = algorithm.to_ascii_uppercase();
    let use_sha256 = alg_upper.starts_with("SHA-256");
    let is_sess = alg_upper.ends_with("-SESS");

    let hash_str = |s: &str| -> String {
        if use_sha256 {
            hex_lower(&sha256_bytes(s.as_bytes()))
        } else {
            hex_lower(&md5_bytes(s.as_bytes()))
        }
    };
    let hash_bytes = |b: &[u8]| -> String {
        if use_sha256 {
            hex_lower(&sha256_bytes(b))
        } else {
            hex_lower(&md5_bytes(b))
        }
    };

    let parsed_url = Url::parse(&normalise_url(raw_url)).ok()?;
    let mut uri = if parsed_url.path().is_empty() {
        "/".to_string()
    } else {
        parsed_url.path().to_string()
    };
    if let Some(q) = parsed_url.query() {
        uri.push('?');
        uri.push_str(q);
    }

    let qop_raw = params.get("qop").map(|s| s.as_str()).unwrap_or("");
    let qop_options: Vec<&str> = qop_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let chosen_qop = if qop_options.contains(&"auth") {
        Some("auth")
    } else if qop_options.contains(&"auth-int") {
        Some("auth-int")
    } else {
        None
    };

    let mut ha1 = hash_str(&format!("{username}:{realm}:{password}"));
    if is_sess {
        ha1 = hash_str(&format!("{ha1}:{nonce}:{cnonce}"));
    }

    let method_upper = method.trim().to_ascii_uppercase();
    let ha2 = if chosen_qop == Some("auth-int") {
        let body_hash = hash_bytes(body);
        hash_str(&format!("{method_upper}:{uri}:{body_hash}"))
    } else {
        hash_str(&format!("{method_upper}:{uri}"))
    };

    let nc = "00000001";
    let response = if let Some(qop) = chosen_qop {
        hash_str(&format!("{ha1}:{nonce}:{nc}:{cnonce}:{qop}:{ha2}"))
    } else {
        hash_str(&format!("{ha1}:{nonce}:{ha2}"))
    };

    let mut parts = vec![
        format!("username=\"{}\"", username.replace('"', "\\\"")),
        format!("realm=\"{}\"", realm.replace('"', "\\\"")),
        format!("nonce=\"{}\"", nonce.replace('"', "\\\"")),
        format!("uri=\"{uri}\""),
        format!("algorithm={algorithm}"),
        format!("response=\"{response}\""),
    ];
    if let Some(qop) = chosen_qop {
        parts.push(format!("qop={qop}"));
        parts.push(format!("nc={nc}"));
        parts.push(format!("cnonce=\"{cnonce}\""));
    }
    if let Some(opaque) = params.get("opaque") {
        parts.push(format!("opaque=\"{}\"", opaque.replace('"', "\\\"")));
    }
    Some(format!("Digest {}", parts.join(", ")))
}

// --------------------------------------------------------------- NTLM Auth

const NTLM_FLAGS: u32 = 0x0008_8207; // UNICODE | OEM | REQUEST_TARGET | NTLM | ALWAYS_SIGN | EXTENDED_SESSIONSECURITY

/// Type 1 Negotiate message (`Authorization: NTLM <base64>`).
pub fn ntlm_negotiate_header() -> String {
    let mut msg = Vec::with_capacity(32);
    msg.extend_from_slice(b"NTLMSSP\0");
    msg.extend_from_slice(&1u32.to_le_bytes()); // Type 1
    msg.extend_from_slice(&NTLM_FLAGS.to_le_bytes());
    msg.extend_from_slice(&[0u8; 8]); // Domain security buffer
    msg.extend_from_slice(&[0u8; 8]); // Workstation security buffer
    format!(
        "NTLM {}",
        base64::engine::general_purpose::STANDARD.encode(&msg)
    )
}

fn utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for code in s.encode_utf16() {
        out.extend_from_slice(&code.to_le_bytes());
    }
    out
}

/// Parse a Type 2 Challenge from `WWW-Authenticate: NTLM <base64>` and produce
/// a Type 3 Authenticate header (`Authorization: NTLM <base64>`).
pub fn ntlm_authenticate_header(
    www_authenticate: &str,
    username: &str,
    password: &str,
    domain: &str,
    client_challenge: [u8; 8],
) -> Option<String> {
    let b64 = www_authenticate.trim().strip_prefix("NTLM")?.trim();
    if b64.is_empty() {
        return None;
    }
    let challenge_msg = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    if challenge_msg.len() < 32 || &challenge_msg[0..8] != b"NTLMSSP\0" {
        return None;
    }
    let msg_type = u32::from_le_bytes(challenge_msg[8..12].try_into().ok()?);
    if msg_type != 2 {
        return None;
    }
    let flags = u32::from_le_bytes(challenge_msg[20..24].try_into().ok()?);
    let server_challenge: [u8; 8] = challenge_msg[24..32].try_into().ok()?;

    // Extract TargetInfo if present (offset 40..48 in Type 2)
    let target_info: &[u8] = if challenge_msg.len() >= 48 {
        let len = u16::from_le_bytes(challenge_msg[40..42].try_into().ok()?) as usize;
        let off = u32::from_le_bytes(challenge_msg[44..48].try_into().ok()?) as usize;
        if off + len <= challenge_msg.len() {
            &challenge_msg[off..off + len]
        } else {
            &[]
        }
    } else {
        &[]
    };

    // Compute NTLMv2 hash: HMAC-MD5(MD4(UTF-16LE(password)), UTF-16LE(UPPER(username) + domain))
    let nt_hash = md4_bytes(&utf16le(password));
    let identity = format!("{}{}", username.to_uppercase(), domain);
    let ntlmv2_hash = hmac_md5(&nt_hash, &utf16le(&identity));

    // NTLMv2 client blob
    let mut blob = Vec::new();
    blob.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]); // RespType = 1, HiRespType = 1
    blob.extend_from_slice(&[0u8; 4]); // Reserved
    blob.extend_from_slice(&[0u8; 8]); // Timestamp
    blob.extend_from_slice(&client_challenge);
    blob.extend_from_slice(&[0u8; 4]); // Reserved
    blob.extend_from_slice(target_info);
    blob.extend_from_slice(&[0u8; 4]); // Terminator

    let mut server_and_blob = Vec::with_capacity(8 + blob.len());
    server_and_blob.extend_from_slice(&server_challenge);
    server_and_blob.extend_from_slice(&blob);
    let nt_proof_str = hmac_md5(&ntlmv2_hash, &server_and_blob);

    let mut nt_response = Vec::with_capacity(16 + blob.len());
    nt_response.extend_from_slice(&nt_proof_str);
    nt_response.extend_from_slice(&blob);

    // LMv2 response
    let mut server_and_cc = [0u8; 16];
    server_and_cc[0..8].copy_from_slice(&server_challenge);
    server_and_cc[8..16].copy_from_slice(&client_challenge);
    let lmv2_mac = hmac_md5(&ntlmv2_hash, &server_and_cc);
    let mut lm_response = Vec::with_capacity(24);
    lm_response.extend_from_slice(&lmv2_mac);
    lm_response.extend_from_slice(&client_challenge);

    let domain_bytes = utf16le(domain);
    let user_bytes = utf16le(username);
    let workstation_bytes: &[u8] = &[];
    let session_key_bytes: &[u8] = &[];

    let header_len = 64usize;
    let mut payload = Vec::new();
    let push_buf = |payload: &mut Vec<u8>, data: &[u8]| -> [u8; 8] {
        let offset = (header_len + payload.len()) as u32;
        let len = data.len() as u16;
        payload.extend_from_slice(data);
        let mut sec = [0u8; 8];
        sec[0..2].copy_from_slice(&len.to_le_bytes());
        sec[2..4].copy_from_slice(&len.to_le_bytes());
        sec[4..8].copy_from_slice(&offset.to_le_bytes());
        sec
    };

    let lm_sec = push_buf(&mut payload, &lm_response);
    let nt_sec = push_buf(&mut payload, &nt_response);
    let dom_sec = push_buf(&mut payload, &domain_bytes);
    let usr_sec = push_buf(&mut payload, &user_bytes);
    let wks_sec = push_buf(&mut payload, workstation_bytes);
    let key_sec = push_buf(&mut payload, session_key_bytes);

    let mut msg = Vec::with_capacity(header_len + payload.len());
    msg.extend_from_slice(b"NTLMSSP\0");
    msg.extend_from_slice(&3u32.to_le_bytes()); // Type 3
    msg.extend_from_slice(&lm_sec);
    msg.extend_from_slice(&nt_sec);
    msg.extend_from_slice(&dom_sec);
    msg.extend_from_slice(&usr_sec);
    msg.extend_from_slice(&wks_sec);
    msg.extend_from_slice(&key_sec);
    msg.extend_from_slice(&(flags & NTLM_FLAGS).to_le_bytes());
    msg.extend_from_slice(&payload);

    Some(format!(
        "NTLM {}",
        base64::engine::general_purpose::STANDARD.encode(&msg)
    ))
}

// ------------------------------------------------ OAuth 2.0 private_key_jwt

/// Minimal big-unsigned-integer arithmetic for RSA PKCS#1 v1.5 signing.
#[derive(Clone, PartialEq, Eq)]
struct BigUint {
    /// Little-endian 32-bit limbs.
    limbs: Vec<u32>,
}

impl BigUint {
    fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut limbs = Vec::new();
        let mut i = bytes.len();
        while i > 0 {
            let start = i.saturating_sub(4);
            let mut buf = [0u8; 4];
            let slice = &bytes[start..i];
            buf[4 - slice.len()..].copy_from_slice(slice);
            limbs.push(u32::from_be_bytes(buf));
            i = start;
        }
        let mut b = Self { limbs };
        b.trim();
        b
    }

    fn to_be_bytes_padded(&self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for (idx, &limb) in self.limbs.iter().enumerate() {
            let be = limb.to_be_bytes();
            for (b_idx, &byte) in be.iter().rev().enumerate() {
                let pos = idx * 4 + b_idx;
                if pos < len {
                    out[len - 1 - pos] = byte;
                }
            }
        }
        out
    }

    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for (&a, &b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
            if a != b {
                return a.cmp(&b);
            }
        }
        std::cmp::Ordering::Equal
    }

    fn sub_assign(&mut self, other: &Self) {
        let mut borrow: i64 = 0;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = *other.limbs.get(i).unwrap_or(&0) as i64;
            let diff = a - b - borrow;
            if diff < 0 {
                self.limbs[i] = (diff + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                self.limbs[i] = diff as u32;
                borrow = 0;
            }
        }
        self.trim();
    }

    fn shl1(&mut self) {
        let mut carry = 0u32;
        for limb in &mut self.limbs {
            let next = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            self.limbs.push(carry);
        }
    }

    fn bit(&self, idx: usize) -> bool {
        let limb = idx / 32;
        let bit = idx % 32;
        self.limbs.get(limb).is_some_and(|v| (v >> bit) & 1 == 1)
    }

    fn bit_len(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(&last) => (self.limbs.len() - 1) * 32 + (32 - last.leading_zeros() as usize),
        }
    }

    fn rem(&self, modulus: &Self) -> Self {
        let mut r = Self { limbs: Vec::new() };
        for i in (0..self.bit_len()).rev() {
            r.shl1();
            if self.bit(i) {
                if r.limbs.is_empty() {
                    r.limbs.push(1);
                } else {
                    r.limbs[0] |= 1;
                }
            }
            if r.cmp(modulus) != std::cmp::Ordering::Less {
                r.sub_assign(modulus);
            }
        }
        r
    }

    fn mul_mod(&self, other: &Self, modulus: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self { limbs: Vec::new() };
        }
        let mut prod = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let cur = prod[i + j] as u64 + (a as u64) * (b as u64) + carry;
                prod[i + j] = cur as u32;
                carry = cur >> 32;
            }
            prod[i + other.limbs.len()] = carry as u32;
        }
        let mut p = Self { limbs: prod };
        p.trim();
        p.rem(modulus)
    }

    fn modpow(&self, exp: &Self, modulus: &Self) -> Self {
        let mut result = Self::from_be_bytes(&[1]);
        let mut base = self.rem(modulus);
        let bits = exp.bit_len();
        for i in 0..bits {
            if exp.bit(i) {
                result = result.mul_mod(&base, modulus);
            }
            if i + 1 < bits {
                base = base.mul_mod(&base, modulus);
            }
        }
        result
    }
}

fn parse_der_element(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    if input.len() < 2 {
        return None;
    }
    let tag = input[0];
    let len_byte = input[1];
    let (len, hdr_len) = if len_byte & 0x80 == 0 {
        (len_byte as usize, 2usize)
    } else {
        let num_bytes = (len_byte & 0x7f) as usize;
        if num_bytes == 0 || num_bytes > 4 || input.len() < 2 + num_bytes {
            return None;
        }
        let mut l = 0usize;
        for &b in &input[2..2 + num_bytes] {
            l = (l << 8) | (b as usize);
        }
        (l, 2 + num_bytes)
    };
    if input.len() < hdr_len + len {
        return None;
    }
    Some((tag, &input[hdr_len..hdr_len + len], &input[hdr_len + len..]))
}

/// Extract `(modulus_n, private_exponent_d, modulus_byte_len)` from a PKCS#1 or
/// PKCS#8 RSA private key in PEM format.
fn parse_rsa_private_key_pem(pem: &str) -> Result<(BigUint, BigUint, usize), String> {
    let b64: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("-----"))
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("invalid base64 in private key PEM: {e}"))?;

    let (tag, seq, _) = parse_der_element(&der).ok_or("invalid ASN.1 DER in private key")?;
    if tag != 0x30 {
        return Err("expected ASN.1 SEQUENCE in private key".into());
    }
    // Check if PKCS#8 (version, AlgorithmIdentifier SEQUENCE, PrivateKey OCTET STRING)
    let (t0, _ver, rest) = parse_der_element(seq).ok_or("truncated ASN.1 SEQUENCE")?;
    if t0 != 0x02 {
        return Err("expected version INTEGER in private key".into());
    }
    let (t1, elem1, rest2) = parse_der_element(rest).ok_or("truncated private key")?;
    let rsa_seq = if t1 == 0x30 {
        // PKCS#8: next element is OCTET STRING (0x04) containing PKCS#1 RSAPrivateKey SEQUENCE
        let (t2, octet, _) =
            parse_der_element(rest2).ok_or("missing PKCS#8 privateKey octet string")?;
        if t2 != 0x04 {
            return Err("expected OCTET STRING in PKCS#8 private key".into());
        }
        let (t_inner, inner_seq, _) =
            parse_der_element(octet).ok_or("invalid inner RSAPrivateKey")?;
        if t_inner != 0x30 {
            return Err("expected RSAPrivateKey SEQUENCE inside PKCS#8".into());
        }
        let (_, _, after_ver) = parse_der_element(inner_seq).ok_or("truncated RSAPrivateKey")?;
        after_ver
    } else if t1 == 0x02 {
        // PKCS#1: elem1 is already modulus `n`! Re-parse from `rest`.
        let _ = elem1;
        rest
    } else {
        return Err("unrecognized private key structure".into());
    };

    let (tn, mut n_bytes, rest) = parse_der_element(rsa_seq).ok_or("missing RSA modulus n")?;
    let (te, _e_bytes, rest) = parse_der_element(rest).ok_or("missing RSA publicExponent e")?;
    let (td, mut d_bytes, _) = parse_der_element(rest).ok_or("missing RSA privateExponent d")?;
    if tn != 0x02 || te != 0x02 || td != 0x02 {
        return Err("expected INTEGER elements in RSAPrivateKey".into());
    }
    while n_bytes.first() == Some(&0) {
        n_bytes = &n_bytes[1..];
    }
    while d_bytes.first() == Some(&0) {
        d_bytes = &d_bytes[1..];
    }
    let k = n_bytes.len();
    if k < 64 {
        return Err("RSA key modulus is too short".into());
    }
    Ok((
        BigUint::from_be_bytes(n_bytes),
        BigUint::from_be_bytes(d_bytes),
        k,
    ))
}

fn rsa_pkcs1_v15_sha256_sign(pem: &str, message: &[u8]) -> Result<Vec<u8>, String> {
    let (n, d, k) = parse_rsa_private_key_pem(pem)?;
    // ASN.1 DigestInfo prefix for SHA-256 + 32-byte digest = 51 bytes
    const SHA256_DIGEST_INFO_PREFIX: [u8; 19] = [
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    let digest = sha256_bytes(message);
    let t_len = SHA256_DIGEST_INFO_PREFIX.len() + digest.len();
    if k < t_len + 11 {
        return Err("RSA modulus too short for PKCS#1 v1.5 SHA-256 padding".into());
    }
    let mut em = Vec::with_capacity(k);
    em.push(0x00);
    em.push(0x01);
    em.resize(k - t_len - 1, 0xff);
    em.push(0x00);
    em.extend_from_slice(&SHA256_DIGEST_INFO_PREFIX);
    em.extend_from_slice(&digest);

    let m = BigUint::from_be_bytes(&em);
    let s = m.modpow(&d, &n);
    Ok(s.to_be_bytes_padded(k))
}

/// Build a signed RFC 7523 `private_key_jwt` assertion for OAuth 2.0 client
/// authentication. Supports RSA private keys in PEM format (`RS256`) as well as
/// symmetric secrets (`HS256`).
pub fn build_client_assertion_jwt(
    client_id: &str,
    audience: &str,
    private_key_or_secret: &str,
    key_id: &str,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let key_trimmed = private_key_or_secret.trim();
    if key_trimmed.is_empty() {
        return Err("private key (or client secret) is required for private_key_jwt".into());
    }
    let is_rsa = key_trimmed.contains("-----BEGIN");
    let alg = if is_rsa { "RS256" } else { "HS256" };

    let mut header = serde_json::json!({
        "alg": alg,
        "typ": "JWT",
    });
    if !key_id.trim().is_empty() {
        header["kid"] = serde_json::Value::String(key_id.trim().to_string());
    }
    let iat = now.timestamp();
    let claims = serde_json::json!({
        "iss": client_id,
        "sub": client_id,
        "aud": audience,
        "jti": new_id(),
        "iat": iat,
        "exp": iat + 300,
    });

    let b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let signing_input = format!(
        "{}.{}",
        b64url.encode(header.to_string()),
        b64url.encode(claims.to_string())
    );

    let sig_bytes = if is_rsa {
        rsa_pkcs1_v15_sha256_sign(key_trimmed, signing_input.as_bytes())?
    } else {
        hmac_sha256(key_trimmed.as_bytes(), signing_input.as_bytes()).to_vec()
    };

    Ok(format!("{signing_input}.{}", b64url.encode(sig_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn aws_sigv4_adds_expected_headers_and_signature() {
        let mut headers = Vec::new();
        let now = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        sign_aws_sigv4(
            "GET",
            "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            &mut headers,
            b"",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1",
            "iam",
            "",
            now,
        )
        .unwrap();
        let auth = headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("authorization"))
            .unwrap();
        assert!(auth
            .value
            .starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20260501/us-east-1/iam/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="));
    }

    #[test]
    fn digest_auth_computes_rfc7616_md5_and_sha256_responses() {
        let challenge = r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#;
        let hdr = digest_authorization_header(
            challenge,
            "GET",
            "http://example.com/dir/index.html",
            b"",
            "Mufasa",
            "Circle Of Life",
            "0a4f113b",
        )
        .unwrap();
        // Classic RFC 2617 / 7616 test vector response hash: 6629fae49393a05397450978507c4ef1
        assert!(
            hdr.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{hdr}"
        );
        assert!(hdr.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));

        let sha_challenge = r#"Digest realm="api", qop="auth", algorithm=SHA-256, nonce="n1""#;
        let sha_hdr = digest_authorization_header(
            sha_challenge,
            "POST",
            "https://example.com/v1/items",
            b"{}",
            "alice",
            "secret",
            "c1",
        )
        .unwrap();
        assert!(sha_hdr.contains("algorithm=SHA-256"));
        assert!(sha_hdr.contains("qop=auth"));
    }

    #[test]
    fn ntlm_handshake_round_trip() {
        let type1 = ntlm_negotiate_header();
        assert!(type1.starts_with("NTLM TlRMTVNTUAAB"));

        // Build a synthetic Type 2 challenge message
        let mut type2 = Vec::new();
        type2.extend_from_slice(b"NTLMSSP\0");
        type2.extend_from_slice(&2u32.to_le_bytes());
        type2.extend_from_slice(&[0u8; 8]); // target name
        type2.extend_from_slice(&NTLM_FLAGS.to_le_bytes());
        type2.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]); // server challenge
        type2.extend_from_slice(&[0u8; 8]); // context
        type2.extend_from_slice(&[0u8; 8]); // target info
        let challenge_hdr = format!(
            "NTLM {}",
            base64::engine::general_purpose::STANDARD.encode(&type2)
        );

        let type3 =
            ntlm_authenticate_header(&challenge_hdr, "User", "Password", "DOMAIN", [0xaa; 8])
                .unwrap();
        assert!(type3.starts_with("NTLM TlRMTVNTUAAD"));
    }

    #[test]
    fn private_key_jwt_builds_valid_jwt() {
        let now = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let jwt = build_client_assertion_jwt(
            "my-client",
            "https://idp.example.com/oauth/token",
            "super-secret-signing-key",
            "kid-1",
            now,
        )
        .unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
    }
}
