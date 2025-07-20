use m3u8::tag::hls::map::MapByterange;
use std::{error::Error, fmt::Display};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    DomException, Request, Response,
    js_sys::{ArrayBuffer, TypeError, Uint8Array},
};

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct FetchTextResponse {
    pub response_text: String,
}
impl FetchTextResponse {
    fn empty() -> Self {
        Self {
            response_text: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchArrayBufferResonse {
    pub response_body: Vec<u8>,
    pub content_type: Option<String>,
    pub url: String,
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
    let response_text = JsFuture::from(response.text().map_err(fetch_failed)?)
        .await
        .map_err(fetch_failed)?
        .as_string()
        .expect("text() on a fetch Response must provide a String");
    Ok(FetchTextResponse { response_text })
}

pub async fn fetch_array_buffer(
    request_url: String,
    byterange: Option<RequestRange>,
) -> Result<FetchArrayBufferResonse, FetchError> {
    let response = response_from(&request_url, byterange).await?;
    let content_type = content_type_from(&response);
    let url = response.url();
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
    response.headers().get("Content-Type").ok().flatten()
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
