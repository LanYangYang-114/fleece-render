use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use percent_encoding::percent_decode_str;
use url::Url;

use crate::config::AppConfig;
use crate::error::{ApiError, ApiResult};

const DATA_URL_PREFIX: &str = "data:";

fn enforce_limit(bytes: Vec<u8>, role: &str, limit: usize) -> ApiResult<Vec<u8>> {
    if bytes.len() > limit {
        return Err(texture_limit_error(role, limit));
    }
    Ok(bytes)
}

fn texture_limit_error(role: &str, limit: usize) -> ApiError {
    ApiError::texture(format!("{role} exceeds the {limit} byte limit"))
}

pub async fn bytes_from_source(
    source: &str,
    role: &str,
    client: &reqwest::Client,
    config: &AppConfig,
) -> ApiResult<Vec<u8>> {
    let source = source.trim();
    if source.is_empty() {
        return Err(ApiError::texture(format!("{role} source is empty")));
    }

    if source.starts_with(DATA_URL_PREFIX) {
        return decode_data_url(source, role, config.render.max_texture_bytes);
    }
    if let Some(rest) = source.strip_prefix("//") {
        return download(&format!("https:{rest}"), role, client, config).await;
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        return download(source, role, client, config).await;
    }
    if source.starts_with("file://") {
        let url = Url::parse(source)
            .map_err(|error| ApiError::texture(format!("invalid {role} file URL: {error}")))?;
        let path = url
            .to_file_path()
            .map_err(|_| ApiError::texture(format!("invalid {role} file URL path")))?;
        return read_file(&path, role, config);
    }

    let path = Path::new(source);
    if path.is_file() {
        return read_file(path, role, config);
    }

    if looks_like_base64(source) {
        return enforce_limit(
            decode_base64_text(source, role)?,
            role,
            config.render.max_texture_bytes,
        );
    }

    Err(ApiError::texture(format!(
        "{role} is neither an existing file, URL, data URL, nor base64 image"
    )))
}

pub fn decode_request_body(body: &[u8], role: &str, config: &AppConfig) -> ApiResult<Vec<u8>> {
    if body.is_empty() {
        return Err(ApiError::texture(format!("{role} body is empty")));
    }
    if body.starts_with(b"\x89PNG\r\n\x1a\n")
        || body.starts_with(b"\xff\xd8\xff")
        || body.starts_with(b"RIFF")
    {
        if body.len() > config.render.max_texture_bytes {
            return Err(texture_limit_error(role, config.render.max_texture_bytes));
        }
        return Ok(body.to_vec());
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| ApiError::texture(format!("{role} body is not image bytes")))?;
    let text = text.trim();
    if text.starts_with(DATA_URL_PREFIX) {
        return decode_data_url(text, role, config.render.max_texture_bytes);
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        return Err(ApiError::texture(format!(
            "{role} body contains a URL; use the query parameter form for network URLs"
        )));
    }
    enforce_limit(
        decode_base64_text(text, role)?,
        role,
        config.render.max_texture_bytes,
    )
}

async fn download(
    url: &str,
    role: &str,
    client: &reqwest::Client,
    config: &AppConfig,
) -> ApiResult<Vec<u8>> {
    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs_f64(
            config.network.texture_timeout_seconds,
        ))
        .send()
        .await
        .map_err(|error| {
            ApiError::upstream(format!(
                "failed to download {role}: {}",
                error.without_url()
            ))
        })?;
    if !response.status().is_success() {
        return Err(ApiError::upstream(format!(
            "failed to download {role}: HTTP {}",
            response.status()
        )));
    }
    let limit = config.render.max_texture_bytes;
    read_response_limited(response, role, limit, || texture_limit_error(role, limit)).await
}

/// Apply a byte limit while consuming the body, including decoded HTTP bodies
/// without a Content-Length. A post-download size check cannot cap memory use.
pub(crate) async fn read_response_limited(
    mut response: reqwest::Response,
    role: &str,
    limit: usize,
    on_limit: impl Fn() -> ApiError,
) -> ApiResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(on_limit());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        ApiError::upstream(format!("failed to read {role}: {}", error.without_url()))
    })? {
        if chunk.len() > limit - bytes.len() {
            return Err(on_limit());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn read_file(path: &Path, role: &str, config: &AppConfig) -> ApiResult<Vec<u8>> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| ApiError::texture(format!("cannot stat {role} file: {error}")))?;
    if !metadata.is_file() {
        return Err(ApiError::texture(format!("{role} is not a regular file")));
    }
    let limit = config.render.max_texture_bytes;
    if metadata.len() > limit as u64 {
        return Err(texture_limit_error(role, limit));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| {
            file.take((limit as u64).saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|error| ApiError::texture(format!("cannot read {role} file: {error}")))?;
    enforce_limit(bytes, role, limit)
}

fn decode_data_url(source: &str, role: &str, limit: usize) -> ApiResult<Vec<u8>> {
    let (header, payload) = source
        .split_once(',')
        .ok_or_else(|| ApiError::texture(format!("malformed {role} data URL")))?;
    let bytes = if header.to_ascii_lowercase().contains(";base64") {
        decode_base64_text(payload, role)?
    } else {
        percent_decode_str(payload)
            .take(limit.saturating_add(1))
            .collect()
    };
    if bytes.is_empty() {
        return Err(ApiError::texture(format!("{role} data URL is empty")));
    }
    enforce_limit(bytes, role, limit)
}

fn decode_base64_text(source: &str, role: &str) -> ApiResult<Vec<u8>> {
    let cleaned: String = source
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.as_bytes()))
        .map_err(|error| ApiError::texture(format!("invalid {role} base64: {error}")))
}

fn looks_like_base64(source: &str) -> bool {
    let compact: String = source
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    compact.len() >= 64
        && compact
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn decoded_texture_limit_is_the_same_for_all_inline_encodings() {
        let mut config = AppConfig::default();
        let bytes = vec![0xa5; 64];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:image/png;base64,{encoded}");
        let client = reqwest::Client::new();

        config.render.max_texture_bytes = bytes.len();
        for input in [&encoded, &data_url] {
            assert_eq!(
                bytes_from_source(input, "skin", &client, &config)
                    .await
                    .unwrap(),
                bytes
            );
            assert_eq!(
                decode_request_body(input.as_bytes(), "skin", &config).unwrap(),
                bytes
            );
        }

        config.render.max_texture_bytes -= 1;
        for input in [&encoded, &data_url] {
            assert_eq!(
                bytes_from_source(input, "skin", &client, &config)
                    .await
                    .unwrap_err()
                    .code,
                "texture"
            );
            assert_eq!(
                decode_request_body(input.as_bytes(), "skin", &config)
                    .unwrap_err()
                    .code,
                "texture"
            );
        }
    }

    #[tokio::test]
    async fn texture_download_rejects_over_limit_chunked_bodies() {
        use std::io::Write;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut reader = std::io::BufReader::new(&mut socket);
            let mut line = String::new();
            loop {
                line.clear();
                if std::io::BufRead::read_line(&mut reader, &mut line).unwrap() == 0
                    || line == "\r\n"
                {
                    break;
                }
            }
            let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n1234\r\n4\r\n5678\r\n0\r\n\r\n";
            socket.write_all(response).unwrap();
        });
        let mut config = AppConfig::default();
        config.render.max_texture_bytes = 7;
        let result = bytes_from_source(
            &format!("http://{addr}/"),
            "skin",
            &reqwest::Client::builder().no_proxy().build().unwrap(),
            &config,
        )
        .await;
        task.join().unwrap();
        assert_eq!(result.unwrap_err().code, "texture");
    }
}
