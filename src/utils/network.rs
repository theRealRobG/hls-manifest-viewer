use crate::utils::timing::{AppClockSpan, ResourceTimingSample};
use quick_m3u8::tag::hls::MapByterange;
use std::{error::Error, fmt::Display};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortSignal, DomException, Performance, PerformanceResourceTiming, Request, RequestCache,
    RequestInit, Response,
    js_sys::{ArrayBuffer, TypeError, Uint8Array},
};

/// A byte range of one resource. Hashable because a low-latency playlist routinely names
/// several parts inside a single file, so a range is part of what identifies an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestRange {
    pub start: u64,
    pub end: u64,
}
impl RequestRange {
    pub fn from_length_with_offset(length: u64, offset: u64) -> Self {
        Self {
            start: offset,
            end: (offset + length) - 1,
        }
    }

    pub fn range_header_value(&self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }
}
impl From<MapByterange> for RequestRange {
    fn from(value: MapByterange) -> Self {
        Self::from_length_with_offset(value.length, value.offset)
    }
}
impl Display for RequestRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FetchTextResponse {
    pub response_text: String,
    /// Final response URL after redirects (may differ from the request URL).
    pub final_url: String,
    pub content_encoding: Option<String>,
    /// `Content-Length` as the server sent it, which is the encoded length: a gzip
    /// response declares fewer bytes here than the text this struct carries.
    pub content_length: Option<u64>,
    pub last_modified: Option<String>,
    pub date: Option<String>,
}
impl FetchTextResponse {
    fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct FetchArrayBufferResonse {
    pub response_body: Vec<u8>,
    pub content_type: Option<String>,
    pub url: String,
    /// HTTP status code. A ranged request only got its range when this is 206.
    pub status: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchError {
    pub error: String,
    pub extra_info: Option<String>,
}
impl Error for FetchError {}
impl Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(extra_info) = &self.extra_info {
            write!(f, "{}\n{}", self.error, extra_info)
        } else {
            write!(f, "{}", self.error)
        }
    }
}

pub async fn fetch_text(request_url: String) -> Result<FetchTextResponse, FetchError> {
    if request_url.is_empty() {
        return Ok(FetchTextResponse::empty());
    }
    let response = response_from(&request_url, None).await?;
    let final_url = response.url();
    let content_encoding = header_get(&response, "Content-Encoding");
    let content_length =
        header_get(&response, "Content-Length").and_then(|v| v.trim().parse::<u64>().ok());
    let last_modified = header_get(&response, "Last-Modified");
    let date = header_get(&response, "Date");
    let response_text = JsFuture::from(response.text().map_err(fetch_failed)?)
        .await
        .map_err(fetch_failed)?
        .as_string()
        .expect("text() on a fetch Response must provide a String");
    Ok(FetchTextResponse {
        response_text,
        final_url,
        content_encoding,
        content_length,
        last_modified,
        date,
    })
}

pub async fn fetch_array_buffer(
    request_url: String,
    byterange: Option<RequestRange>,
) -> Result<FetchArrayBufferResonse, FetchError> {
    let response = response_from(&request_url, byterange).await?;
    let content_type = content_type_from(&response);
    let url = response.url();
    let status = response.status();
    let response_buf = JsFuture::from(response.array_buffer().map_err(fetch_failed)?)
        .await
        .map_err(fetch_failed)?;
    let array_buf = response_buf
        .dyn_into::<ArrayBuffer>()
        .expect("array_buffer() on a fetch Response must provide an ArrayBuffer");
    let data = Uint8Array::new(&array_buf);
    let mut body = vec![0; data.length() as usize];
    data.copy_to(&mut body);
    Ok(FetchArrayBufferResonse {
        response_body: body,
        content_type,
        url,
        status,
    })
}

async fn response_from(
    request_url: &str,
    byterange: Option<RequestRange>,
) -> Result<Response, FetchError> {
    let window = web_sys::window().expect("Window must be defined");
    let request = Request::new_with_str(request_url).map_err(fetch_failed)?;
    if let Some(byterange) = byterange {
        request
            .headers()
            .set("Range", &byterange.range_header_value())
            .map_err(fetch_failed)?;
    }
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(fetch_failed)?;
    let response: Response = response
        .dyn_into()
        .expect("Fetch must resolve to a Response");
    validate(&response).await?;
    Ok(response)
}

fn fetch_failed(e: JsValue) -> FetchError {
    match e.dyn_into::<TypeError>() {
        Ok(e) => FetchError {
            error: String::from(e.to_string()),
            extra_info: None,
        },
        Err(e) => match e.dyn_into::<DomException>() {
            Ok(e) => FetchError {
                error: String::from(e.to_string()),
                extra_info: None,
            },
            Err(e) => FetchError {
                error: format!("Fetch failed: {e:?}"),
                extra_info: None,
            },
        },
    }
}

fn content_type_from(response: &Response) -> Option<String> {
    header_get(response, "Content-Type")
}

fn header_get(response: &Response, name: &str) -> Option<String> {
    response.headers().get(name).ok().flatten()
}

// ── Measured fetches ─────────────────────────────────────────────────────────
//
// These sit beside the plain fetches above rather than replacing them: Author, Validate
// and the Viewer keep the caching and header behaviour they have always had, and only the
// Inspect Timing phase asks for `no-store`, an abort signal and a clock reading.

/// Why a measured fetch produced no measurement.
///
/// A run the user stopped is not a failure and must not be reported as one, and a missing
/// `Performance` is not a duration of zero — both get their own variant so the results
/// table can say what actually happened.
#[derive(Debug, Clone, PartialEq)]
pub enum MeasuredFetchError {
    /// The abort signal fired: either Stop, or a new probe superseding this run.
    Cancelled,
    /// `performance.now()` is unavailable, so nothing here can be timed honestly.
    ClockUnavailable,
    /// The server answered, but not with a status the request can be measured against.
    HttpStatus { status: u16, status_text: String },
    /// The fetch itself did not complete: network error, CORS, or a URL the browser
    /// refused to build a request from.
    Failed(FetchError),
}

impl MeasuredFetchError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl Error for MeasuredFetchError {}
impl Display for MeasuredFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Cancelled"),
            Self::ClockUnavailable => {
                write!(f, "performance.now() is unavailable, so nothing was timed")
            }
            Self::HttpStatus {
                status,
                status_text,
            } => write!(f, "HTTP {status} {status_text}"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// What one measured fetch observed.
///
/// `body_bytes` is what arrived in the app after any decompression; `content_length` is
/// the encoded length the server declared. They differ on a compressed response, so any
/// throughput figure has to name which one it divided by.
#[derive(Debug, Clone)]
pub struct MeasuredFetch {
    /// Where the response came from. A redirect's cost sits inside the measured time and
    /// cannot be separated from it, so a row whose final URL differs has to say so.
    pub final_url: String,
    pub status: u16,
    pub body_bytes: u64,
    pub content_length: Option<u64>,
    /// App-observed timestamps. These include JS scheduling and, for a byte body, the
    /// copy out of the `Uint8Array`, so they are not a network measurement.
    pub app: AppClockSpan,
    /// The browser's own measurement, when an entry could be matched to this request.
    pub resource_timing: Option<ResourceTimingSample>,
}

enum BodyKind {
    Text,
    Bytes,
}

enum MeasuredBody {
    Text(String),
    Bytes(Vec<u8>),
}

/// Fetch a playlist with `cache: no-store` and time it.
pub async fn fetch_text_measured(
    request_url: String,
    signal: Option<&AbortSignal>,
) -> Result<(MeasuredFetch, String), MeasuredFetchError> {
    let (measured, body) = fetch_measured(request_url, None, signal, BodyKind::Text).await?;
    let MeasuredBody::Text(text) = body else {
        unreachable!("a text body was requested");
    };
    Ok((measured, text))
}

/// Fetch media bytes with `cache: no-store` and time it. The body is returned so a caller
/// can count what actually arrived; nothing here decrypts or parses it.
pub async fn fetch_bytes_measured(
    request_url: String,
    byterange: Option<RequestRange>,
    signal: Option<&AbortSignal>,
) -> Result<(MeasuredFetch, Vec<u8>), MeasuredFetchError> {
    let (measured, body) = fetch_measured(request_url, byterange, signal, BodyKind::Bytes).await?;
    let MeasuredBody::Bytes(bytes) = body else {
        unreachable!("a byte body was requested");
    };
    Ok((measured, bytes))
}

async fn fetch_measured(
    request_url: String,
    byterange: Option<RequestRange>,
    signal: Option<&AbortSignal>,
    want: BodyKind,
) -> Result<(MeasuredFetch, MeasuredBody), MeasuredFetchError> {
    let window = web_sys::window().expect("Window must be defined");
    // No clock, no measurement. Reporting zeros here would be the one thing the Timing
    // phase must never do.
    let performance = window
        .performance()
        .ok_or(MeasuredFetchError::ClockUnavailable)?;

    let init = RequestInit::new();
    // The measurement is of a request that reached the origin or its CDN. A cache hit in
    // the browser would time the disk instead.
    init.set_cache(RequestCache::NoStore);
    init.set_signal(signal);
    let request = Request::new_with_str_and_init(&request_url, &init)
        .map_err(|e| measured_failure(e, signal))?;
    if let Some(byterange) = byterange {
        request
            .headers()
            .set("Range", &byterange.range_header_value())
            .map_err(|e| measured_failure(e, signal))?;
    }

    // Entries are matched by URL and by falling inside this window, so the buffer starts
    // empty. Concurrent measured fetches each clear before any of them has completed, so
    // none of them loses its own entry.
    performance.clear_resource_timings();

    let request_ms = performance.now();
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| measured_failure(e, signal))?;
    let headers_ms = performance.now();
    let response: Response = response
        .dyn_into()
        .expect("Fetch must resolve to a Response");

    let status = response.status();
    // A `Range` answered with 200 is not an error — the caller is told about it and can
    // say the server ignored the range — but any other non-success status has no body
    // worth timing.
    if !response.ok() && status != 206 {
        return Err(MeasuredFetchError::HttpStatus {
            status,
            status_text: response.status_text(),
        });
    }

    let final_url = response.url();
    let content_length =
        header_get(&response, "Content-Length").and_then(|v| v.trim().parse::<u64>().ok());

    let body = match want {
        BodyKind::Text => {
            let text = JsFuture::from(response.text().map_err(|e| measured_failure(e, signal))?)
                .await
                .map_err(|e| measured_failure(e, signal))?
                .as_string()
                .expect("text() on a fetch Response must provide a String");
            MeasuredBody::Text(text)
        }
        BodyKind::Bytes => {
            let buf = JsFuture::from(
                response
                    .array_buffer()
                    .map_err(|e| measured_failure(e, signal))?,
            )
            .await
            .map_err(|e| measured_failure(e, signal))?;
            let array_buf = buf
                .dyn_into::<ArrayBuffer>()
                .expect("array_buffer() on a fetch Response must provide an ArrayBuffer");
            let data = Uint8Array::new(&array_buf);
            let mut bytes = vec![0; data.length() as usize];
            data.copy_to(&mut bytes);
            MeasuredBody::Bytes(bytes)
        }
    };
    let body_end_ms = performance.now();

    let body_bytes = match &body {
        MeasuredBody::Text(t) => t.len() as u64,
        MeasuredBody::Bytes(b) => b.len() as u64,
    };

    let resource_timing =
        latest_resource_timing(&performance, &request_url, request_ms, body_end_ms);

    Ok((
        MeasuredFetch {
            final_url,
            status,
            body_bytes,
            content_length,
            app: AppClockSpan {
                request_ms,
                headers_ms,
                body_end_ms,
            },
            resource_timing,
        },
        body,
    ))
}

/// The Resource Timing entry for this request, if the browser recorded one.
///
/// Entries are named by the URL the request was made with, so a redirected fetch is still
/// found under `requested_url`. The window guards keep a stale entry — one this run did
/// not cause — from being read as this request's measurement, and the last match wins
/// because a repeated URL appends.
fn latest_resource_timing(
    performance: &Performance,
    requested_url: &str,
    request_ms: f64,
    body_end_ms: f64,
) -> Option<ResourceTimingSample> {
    // Resource Timing and performance.now() share a time origin, but the entry's
    // startTime is taken a shade before the app reads its clock.
    const TOLERANCE_MS: f64 = 2.0;
    let mut found = None;
    for entry in performance.get_entries_by_type("resource").iter() {
        let Ok(rt) = entry.dyn_into::<PerformanceResourceTiming>() else {
            continue;
        };
        if rt.name() != requested_url {
            continue;
        }
        if rt.start_time() + TOLERANCE_MS < request_ms
            || rt.response_end() > body_end_ms + TOLERANCE_MS
        {
            continue;
        }
        found = Some(resource_timing_sample(&rt));
    }
    found
}

fn resource_timing_sample(rt: &PerformanceResourceTiming) -> ResourceTimingSample {
    ResourceTimingSample {
        start_time: rt.start_time(),
        request_start: rt.request_start(),
        response_start: rt.response_start(),
        response_end: rt.response_end(),
        connect_start: rt.connect_start(),
        connect_end: rt.connect_end(),
        secure_connection_start: rt.secure_connection_start(),
        domain_lookup_start: rt.domain_lookup_start(),
        domain_lookup_end: rt.domain_lookup_end(),
        encoded_body_size: rt.encoded_body_size(),
        transfer_size: rt.transfer_size(),
        next_hop_protocol: rt.next_hop_protocol(),
    }
}

/// Classify a rejected promise. An `AbortError` is the user stopping the run, and turning
/// it into a network failure would invent an error the network never reported.
fn measured_failure(e: JsValue, signal: Option<&AbortSignal>) -> MeasuredFetchError {
    if signal.is_some_and(|s| s.aborted()) {
        return MeasuredFetchError::Cancelled;
    }
    if let Some(dom) = e.dyn_ref::<DomException>()
        && dom.name() == "AbortError"
    {
        return MeasuredFetchError::Cancelled;
    }
    MeasuredFetchError::Failed(fetch_failed(e))
}

async fn validate(response: &Response) -> Result<(), FetchError> {
    if response.ok() || response.status() == 206 {
        return Ok(());
    }
    let error = format!(
        "Bad HTTP status code: {} {}",
        response.status(),
        response.status_text()
    );
    let Some(content_type) = content_type_from(response) else {
        return Err(FetchError {
            error,
            extra_info: None,
        });
    };
    if content_type.contains("text/plain")
        || content_type.contains("application/json")
        || content_type.contains("application/x-www-form-urlencoded")
    {
        let Ok(response_text_promise) = response.text() else {
            return Err(FetchError {
                error,
                extra_info: None,
            });
        };
        let Ok(text) = JsFuture::from(response_text_promise).await else {
            return Err(FetchError {
                error,
                extra_info: None,
            });
        };
        let extra_info = text.as_string();
        Err(FetchError { error, extra_info })
    } else {
        Err(FetchError {
            error,
            extra_info: None,
        })
    }
}
