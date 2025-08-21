use std::{error::Error, fmt::Display, io, num::ParseIntError};

use super::{LINE_BREAK_ANYWHERE, LINE_BREAK_WORD, SUPPLEMENTAL_VIEW_CLASS, UNDERLINED};
use crate::{
    components::viewer::error::ViewerError,
    utils::query_codec::{Scte35CommandType, Scte35Context},
};
use leptos::{either::Either, prelude::*};
use scte35::parse_splice_info_section;
use serde_json::to_string_pretty;

const SCTE35_TABLE: &str = "scte35-info-table";

#[component]
pub fn Scte35Viewer(context: Scte35Context) -> impl IntoView {
    let Scte35Context {
        message,
        daterange_id,
        command_type,
    } = context;
    match decode_message(&message) {
        Ok(json) => Either::Left(view! {
            <div class=SUPPLEMENTAL_VIEW_CLASS>
                <table class=SCTE35_TABLE>
                    <tr>
                        <td class=LINE_BREAK_WORD>"ID"</td>
                        <td>{daterange_id}</td>
                    </tr>
                    <tr>
                        <td class=LINE_BREAK_WORD>"Type"</td>
                        <td>
                            {match command_type {
                                Scte35CommandType::Out => "SCTE35-OUT",
                                Scte35CommandType::In => "SCTE35-IN",
                                Scte35CommandType::Cmd => "SCTE35-CMD",
                            }}
                        </td>
                    </tr>
                    <tr>
                        <td class=LINE_BREAK_WORD>"Message"</td>
                        <td class=LINE_BREAK_ANYWHERE>
                            <code>{message}</code>
                        </td>
                    </tr>
                </table>
                <p class=UNDERLINED>"Decoded"</p>
                <pre>{json}</pre>
            </div>
        }),
        Err(e) => {
            let (error, extra_info) = match e {
                DecodeMessageError::Hex(e) => (
                    String::from("Error reading hex string"),
                    Some(format!("{e}")),
                ),
                DecodeMessageError::Scte35(e) => (
                    String::from("Error parsing SCTE35 data"),
                    Some(format!("{e}")),
                ),
                DecodeMessageError::Json(e) => (
                    String::from("Error converting to JSON"),
                    Some(format!("{e}")),
                ),
            };
            Either::Right(view! {
                <div class=SUPPLEMENTAL_VIEW_CLASS>
                    <ViewerError error extra_info />
                </div>
            })
        }
    }
}

fn decode_message(message: &str) -> Result<String, DecodeMessageError> {
    let message = if message.starts_with("0x") || message.starts_with("0X") {
        &message[2..]
    } else {
        message
    };
    let hex = decode_hex(message)?;
    let splice_info_section = parse_splice_info_section(&hex)?;
    let pretty_json = to_string_pretty(&splice_info_section)?;
    Ok(pretty_json)
}

// Directly copied from https://stackoverflow.com/a/52992629/7039100
fn decode_hex(s: &str) -> Result<Vec<u8>, DecodeHexError> {
    if s.len() % 2 != 0 {
        Err(DecodeHexError::OddLength)
    } else {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.into()))
            .collect()
    }
}

#[derive(Debug)]
enum DecodeMessageError {
    Hex(DecodeHexError),
    Scte35(io::Error),
    Json(serde_json::Error),
}
impl Display for DecodeMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeMessageError::Hex(e) => e.fmt(f),
            DecodeMessageError::Scte35(e) => e.fmt(f),
            DecodeMessageError::Json(e) => e.fmt(f),
        }
    }
}
impl Error for DecodeMessageError {}
impl From<DecodeHexError> for DecodeMessageError {
    fn from(value: DecodeHexError) -> Self {
        Self::Hex(value)
    }
}
impl From<io::Error> for DecodeMessageError {
    fn from(value: io::Error) -> Self {
        Self::Scte35(value)
    }
}
impl From<serde_json::Error> for DecodeMessageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DecodeHexError {
    OddLength,
    ParseInt(ParseIntError),
}
impl From<ParseIntError> for DecodeHexError {
    fn from(e: ParseIntError) -> Self {
        DecodeHexError::ParseInt(e)
    }
}
impl Display for DecodeHexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DecodeHexError::OddLength => "input string has an odd number of bytes".fmt(f),
            DecodeHexError::ParseInt(e) => e.fmt(f),
        }
    }
}
impl Error for DecodeHexError {}
