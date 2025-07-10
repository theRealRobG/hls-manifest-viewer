use std::{fmt::Display, error::Error};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{js_sys::TypeError, DomException, Response};

#[derive(Debug, Clone)]
pub struct FetchTextResponse {
    pub request_url: String,
    pub response_text: String,
}
impl FetchTextResponse {
    fn empty(request_url: String) -> Self {
        Self { request_url, response_text: String::new() }
    }
}

#[derive(Debug, Clone)]
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
        return Ok(FetchTextResponse::empty(request_url));
    }
    let response = response_from(&request_url).await?;
    let response_text = JsFuture::from(response.text().map_err(fetch_failed)?)
        .await
        .map_err(fetch_failed)?
        .as_string()
        .expect("text() on a fetch Response must provide a String");
    Ok(FetchTextResponse { request_url, response_text })
}

async fn response_from(request_url: &str) -> Result<Response, FetchError> {
    let window = web_sys::window().expect("Window must be defined");
    let response = JsFuture::from(window.fetch_with_str(&request_url))
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
        Ok(e) => FetchError { error: String::from(e.to_string()), extra_info: None },
        Err(e) => match e.dyn_into::<DomException>() {
            Ok(e) => FetchError { error: String::from(e.to_string()), extra_info: None },
            Err(e) => FetchError { error: format!("Fetch failed: {e:?}"), extra_info: None },
        },
    }
}

async fn validate(response: &Response) -> Result<(), FetchError> {
    if response.ok() {
        return Ok(());
    }
    let error = format!(
        "Bad HTTP status code: {} {}",
        response.status(),
        response.status_text()
    );
    let Ok(Some(content_type)) = response.headers().get("Content-Type") else {
        return Err(FetchError { error, extra_info: None });
    };
    if content_type.contains("text/plain")
        || content_type.contains("application/json")
        || content_type.contains("application/x-www-form-urlencoded")
    {
        let Ok(response_text_promise) = response.text() else {
            return Err(FetchError { error, extra_info: None });
        };
        let Ok(text) = JsFuture::from(response_text_promise).await else {
            return Err(FetchError { error, extra_info: None });
        };
        let extra_info = text.as_string();
        Err(FetchError { error, extra_info })
    } else {
        Err(FetchError { error, extra_info: None })
    }
}