//! X `x-client-transaction-id` generator.
//!
//! Ported from the official algorithm (iSarabjitDhiman/XClientTransaction and
//! the langkor Rust implementation), adapted to our async transport: it fetches
//! the x.com home page + its `ondemand.s` script through our wreq client and
//! session, extracts the verification key + animation frames, and derives a
//! per-request transaction id. Write endpoints (e.g. CreateTweet) reject
//! requests without a valid id, returning a misleading "daily limit" (344).

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};
use wreq::{Client, header};

use crate::TransportError;

const DEFAULT_KEYWORD: &str = "obfiowerehiring";
const ADDITIONAL_RANDOM_NUMBER: u8 = 3;
const EPOCH_OFFSET_SECS: u64 = 1_682_924_400;

/// Cached key material used to sign every request with an `x-client-transaction-id`.
pub(crate) struct ClientTransaction {
    key_bytes: Vec<u8>,
    animation_key: String,
}

impl ClientTransaction {
    /// Fetch and parse the key material once. Expensive (two network requests +
    /// HTML parsing), so callers should cache the result for the app's lifetime.
    pub(crate) async fn init(client: &Client, cookie: &str) -> Result<Self, TransportError> {
        let home_html = fetch_home(client, cookie).await?;
        let (row_index, key_byte_indices) = fetch_indices(client, cookie, &home_html).await?;

        // All network I/O is done; scraper's `Html` is `!Send`, so it must never
        // be held across an `.await` — everything below is synchronous.
        let doc = Html::parse_document(&home_html);
        let key = get_key(&doc)?;
        let key_bytes = STANDARD
            .decode(key)
            .map_err(|error| parse(format!("verification key is not base64: {error}")))?;
        let animation_key = get_animation_key(&key_bytes, &doc, row_index, &key_byte_indices)?;
        Ok(Self {
            key_bytes,
            animation_key,
        })
    }

    /// Derive the transaction id for one request. `method` is the HTTP verb
    /// (e.g. "POST") and `path` is the URL path (e.g. `/i/api/graphql/…/CreateTweet`).
    pub(crate) fn generate_transaction_id(&self, method: &str, path: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let time_now = now.saturating_sub(EPOCH_OFFSET_SECS) as u32;
        let time_bytes = time_now.to_le_bytes();

        let hash_input = format!(
            "{method}!{path}!{time_now}{DEFAULT_KEYWORD}{}",
            self.animation_key
        );
        let mut hasher = Sha256::new();
        hasher.update(hash_input.as_bytes());
        let hash = hasher.finalize();

        let random: u8 = rand::random();
        let mut bytes = Vec::with_capacity(self.key_bytes.len() + 4 + 16 + 1);
        bytes.extend_from_slice(&self.key_bytes);
        bytes.extend_from_slice(&time_bytes);
        bytes.extend_from_slice(&hash[..16]);
        bytes.push(ADDITIONAL_RANDOM_NUMBER);

        let mut out = Vec::with_capacity(bytes.len() + 1);
        out.push(random);
        out.extend(bytes.iter().map(|&byte| byte ^ random));

        STANDARD.encode(out).trim_end_matches('=').to_string()
    }
}

fn parse(message: impl Into<String>) -> TransportError {
    TransportError::Network(message.into())
}

fn net(error: wreq::Error) -> TransportError {
    TransportError::Network(error.to_string())
}

/// GET a URL as text with browser-like headers.
async fn get_text(client: &Client, url: &str, cookie: &str) -> Result<String, TransportError> {
    let response = client
        .get(url)
        .header(header::COOKIE, cookie)
        .header(header::REFERER, "https://x.com/")
        .send()
        .await
        .map_err(net)?;
    response.text().await.map_err(net)
}

/// Fetch the home page, following X's login-migration form/redirect if present.
async fn fetch_home(client: &Client, cookie: &str) -> Result<String, TransportError> {
    let first = get_text(client, "https://x.com/", cookie).await?;
    // parse_migration owns its `Html` and returns only owned data, so no
    // `!Send` value is held across the awaits below.
    match parse_migration(&first) {
        Some(Migration::Redirect(url)) => get_text(client, &url, cookie).await,
        Some(Migration::Form {
            url,
            method,
            fields,
        }) => {
            let builder = if method.eq_ignore_ascii_case("POST") {
                client.post(url.as_str()).form(&fields)
            } else {
                client.get(url.as_str()).query(&fields)
            };
            let response = builder
                .header(header::COOKIE, cookie)
                .send()
                .await
                .map_err(net)?;
            response.text().await.map_err(net)
        }
        None => Ok(first),
    }
}

enum Migration {
    Redirect(String),
    Form {
        url: String,
        method: String,
        fields: Vec<(String, String)>,
    },
}

fn parse_migration(html: &str) -> Option<Migration> {
    let doc = Html::parse_document(html);

    if let Ok(meta_sel) = Selector::parse("meta[http-equiv='refresh']")
        && let Some(content) = doc
            .select(&meta_sel)
            .next()
            .and_then(|meta| meta.value().attr("content"))
        && let Ok(re) =
            Regex::new(r"(https?://(?:www\.)?(?:twitter|x)\.com(?:/x)?/migrate[^\s'\x22]+)")
        && let Some(url) = re.captures(content).and_then(|c| c.get(1))
    {
        return Some(Migration::Redirect(url.as_str().to_string()));
    }

    let form_sel =
        Selector::parse("form[name='f'], form[action='https://x.com/x/migrate']").ok()?;
    let form = doc.select(&form_sel).next()?;
    let url = form
        .value()
        .attr("action")
        .unwrap_or("https://x.com/x/migrate")
        .to_string();
    let method = form.value().attr("method").unwrap_or("POST").to_string();
    let input_sel = Selector::parse("input").ok()?;
    let fields = form
        .select(&input_sel)
        .filter_map(|input| {
            let name = input.value().attr("name")?;
            let value = input.value().attr("value")?;
            Some((name.to_string(), value.to_string()))
        })
        .collect();
    Some(Migration::Form {
        url,
        method,
        fields,
    })
}

/// Extract the `ondemand.s` byte indices: a row index plus the key-byte indices.
async fn fetch_indices(
    client: &Client,
    cookie: &str,
    home_html: &str,
) -> Result<(usize, Vec<usize>), TransportError> {
    let index_re = Regex::new(r#",(\d+):["']ondemand\.s["']"#).map_err(|e| parse(e.to_string()))?;
    let file_index = index_re
        .captures(home_html)
        .and_then(|c| c.get(1))
        .ok_or_else(|| parse("could not find ondemand file index"))?
        .as_str();

    let name_re =
        Regex::new(&format!("{file_index}:\"([0-9a-f]+)\"")).map_err(|e| parse(e.to_string()))?;
    let file_name = name_re
        .captures(home_html)
        .and_then(|c| c.get(1))
        .ok_or_else(|| parse("could not find ondemand file name"))?
        .as_str();

    let url = format!("https://abs.twimg.com/responsive-web/client-web/ondemand.s.{file_name}a.js");
    let ondemand = get_text(client, &url, cookie).await?;

    let indices_re = Regex::new(r"\(\w\[(\d{1,2})\],\s*16\)").map_err(|e| parse(e.to_string()))?;
    let indices: Vec<usize> = indices_re
        .captures_iter(&ondemand)
        .filter_map(|c| c.get(1).and_then(|m| m.as_str().parse().ok()))
        .collect();

    match indices.split_first() {
        Some((&row, rest)) => Ok((row, rest.to_vec())),
        None => Err(parse(
            "could not extract key-byte indices from ondemand file",
        )),
    }
}

fn get_key(doc: &Html) -> Result<String, TransportError> {
    let sel =
        Selector::parse("[name='twitter-site-verification']").map_err(|e| parse(e.to_string()))?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(str::to_string)
        .ok_or_else(|| parse("could not find twitter-site-verification key"))
}

fn get_2d_array(key_bytes: &[u8], doc: &Html) -> Result<Vec<Vec<i32>>, TransportError> {
    let sel = Selector::parse("[id^='loading-x-anim']").map_err(|e| parse(e.to_string()))?;
    let frames: Vec<ElementRef> = doc.select(&sel).collect();

    let frame_index = (key_bytes.get(5).copied().unwrap_or(0) % 4) as usize;
    let frame = frames
        .get(frame_index)
        .ok_or_else(|| parse("animation frame index out of range"))?;

    let first_child = frame
        .children()
        .find_map(ElementRef::wrap)
        .ok_or_else(|| parse("animation frame has no element child"))?;
    let path_elem = first_child
        .children()
        .filter_map(ElementRef::wrap)
        .nth(1)
        .ok_or_else(|| parse("animation frame group has no second path"))?;
    let d_attr = path_elem
        .value()
        .attr("d")
        .ok_or_else(|| parse("animation path missing 'd' attribute"))?;
    let d_content = d_attr
        .get(9..)
        .ok_or_else(|| parse("animation path data too short"))?;

    Ok(d_content
        .split('C')
        .map(|segment| {
            segment
                .replace(|c: char| !c.is_ascii_digit() && c != '-', " ")
                .split_whitespace()
                .filter_map(|s| s.parse::<i32>().ok())
                .collect()
        })
        .collect())
}

fn get_animation_key(
    key_bytes: &[u8],
    doc: &Html,
    row_index: usize,
    key_byte_indices: &[usize],
) -> Result<String, TransportError> {
    const TOTAL_TIME: f64 = 4096.0;

    let row_index_value = (key_bytes
        .get(row_index)
        .copied()
        .ok_or_else(|| parse("row index out of range"))?
        % 16) as usize;

    let frame_time = key_byte_indices
        .iter()
        .map(|&index| (key_bytes.get(index).copied().unwrap_or(0) % 16) as f64)
        .fold(1.0, |acc, val| acc * val);
    let frame_time = js_round(frame_time / 10.0) * 10.0;

    let arr = get_2d_array(key_bytes, doc)?;
    let frame_row = arr
        .get(row_index_value)
        .ok_or_else(|| parse("animation row index out of range"))?;
    if frame_row.len() < 11 {
        return Err(parse("animation frame row too short"));
    }

    Ok(animate(frame_row, frame_time / TOTAL_TIME))
}

fn solve(value: f64, min_val: f64, max_val: f64, rounding: bool) -> f64 {
    let result = value * (max_val - min_val) / 255.0 + min_val;
    if rounding {
        result.floor()
    } else {
        (result * 100.0).round() / 100.0
    }
}

fn animate(frames: &[i32], target_time: f64) -> String {
    let from_color: Vec<f64> = frames[..3]
        .iter()
        .map(|&i| i as f64)
        .chain(std::iter::once(1.0))
        .collect();
    let to_color: Vec<f64> = frames[3..6]
        .iter()
        .map(|&i| i as f64)
        .chain(std::iter::once(1.0))
        .collect();
    let from_rotation = [0.0];
    let to_rotation = [solve(frames[6] as f64, 60.0, 360.0, true)];

    let curves: Vec<f64> = frames[7..]
        .iter()
        .enumerate()
        .map(|(i, &val)| solve(val as f64, is_odd(i as i32), 1.0, false))
        .collect();

    let progress = cubic_get_value(&curves, target_time);
    let color: Vec<f64> = interpolate(&from_color, &to_color, progress)
        .into_iter()
        .map(|v| v.clamp(0.0, 255.0))
        .collect();
    let rotation = interpolate(&from_rotation, &to_rotation, progress);
    let matrix = convert_rotation_to_matrix(rotation[0]);

    let mut parts = Vec::new();
    for value in &color[..color.len() - 1] {
        parts.push(format!("{:x}", value.round() as i32));
    }
    for value in matrix {
        let rounded = (value * 100.0).round() / 100.0;
        let hex = float_to_hex(rounded.abs());
        if let Some(stripped) = hex.strip_prefix('.') {
            parts.push(format!("0.{stripped}").to_lowercase());
        } else if hex.is_empty() {
            parts.push("0".to_string());
        } else {
            parts.push(hex.to_lowercase());
        }
    }
    parts.push("0".to_string());
    parts.push("0".to_string());

    parts.join("").replace(['.', '-'], "")
}

fn cubic_get_value(curves: &[f64], time: f64) -> f64 {
    let calculate = |a: f64, b: f64, m: f64| {
        3.0 * a * (1.0 - m) * (1.0 - m) * m + 3.0 * b * (1.0 - m) * m * m + m * m * m
    };

    if time <= 0.0 {
        let mut start_gradient = 0.0;
        if curves[0] > 0.0 {
            start_gradient = curves[1] / curves[0];
        } else if curves[1] == 0.0 && curves[2] > 0.0 {
            start_gradient = curves[3] / curves[2];
        }
        return start_gradient * time;
    }
    if time >= 1.0 {
        let mut end_gradient = 0.0;
        if curves[2] < 1.0 {
            end_gradient = (curves[3] - 1.0) / (curves[2] - 1.0);
        } else {
            #[expect(
                clippy::float_cmp,
                reason = "the reference cubic algorithm selects this exact endpoint branch at 1.0"
            )]
            let is_exact_end = curves[2] == 1.0;
            if is_exact_end && curves[0] < 1.0 {
                end_gradient = (curves[1] - 1.0) / (curves[0] - 1.0);
            }
        }
        return 1.0 + end_gradient * (time - 1.0);
    }

    let mut start = 0.0;
    let mut end = 1.0;
    let mut mid = 0.0;
    while start < end {
        mid = (start + end) / 2.0;
        let x_est = calculate(curves[0], curves[2], mid);
        if (time - x_est).abs() < 0.00001 {
            return calculate(curves[1], curves[3], mid);
        }
        if x_est < time {
            start = mid;
        } else {
            end = mid;
        }
    }
    calculate(curves[1], curves[3], mid)
}

fn interpolate(from_list: &[f64], to_list: &[f64], f: f64) -> Vec<f64> {
    from_list
        .iter()
        .zip(to_list.iter())
        .map(|(&a, &b)| a * (1.0 - f) + b * f)
        .collect()
}

fn convert_rotation_to_matrix(degrees: f64) -> Vec<f64> {
    let radians = degrees * std::f64::consts::PI / 180.0;
    vec![radians.cos(), -radians.sin(), radians.sin(), radians.cos()]
}

fn is_odd(num: i32) -> f64 {
    if num % 2 == 1 { -1.0 } else { 0.0 }
}

fn js_round(num: f64) -> f64 {
    let fraction = num - num.trunc();
    #[expect(
        clippy::float_cmp,
        reason = "the JavaScript reference rounds the exact negative half tie toward positive infinity"
    )]
    let is_negative_half = fraction == -0.5;
    if is_negative_half {
        num.ceil()
    } else {
        num.round()
    }
}

fn float_to_hex(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    let parse_digit = |value: i64| -> char {
        if value > 9 {
            char::from_u32((value as u32) + 55).unwrap_or('0')
        } else {
            char::from_digit(value as u32, 10).unwrap_or('0')
        }
    };

    let mut result = String::new();
    let mut quotient = x.floor() as i64;
    let mut fraction = x - quotient as f64;

    if quotient == 0 {
        result.push('0');
    } else {
        while quotient > 0 {
            result.insert(0, parse_digit(quotient % 16));
            quotient /= 16;
        }
    }

    if fraction > 0.0 {
        result.push('.');
        while fraction > 0.0 && result.len() <= 20 {
            fraction *= 16.0;
            let integer = fraction.floor() as i64;
            fraction -= integer as f64;
            result.push(parse_digit(integer));
        }
    }
    result
}
