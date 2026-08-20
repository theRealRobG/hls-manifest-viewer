//! The Inspect Timing controls and results table.
//!
//! Presentation only: everything shown here is computed in [`crate::utils::timing`]. The
//! one rule this file enforces is that an absent measurement renders as an em dash with a
//! reason, never as `0 ms`.

use crate::utils::timing::{
    MAX_MEDIA_PAIRS, MAX_PLAYLIST_REQUESTS, MIN_MEDIA_PAIRS, MIN_PLAYLIST_REQUESTS, MedianRow,
    RowOutcome, TimingProgress, TimingReport, TimingRow, TotalSource, clamp_count, medians,
    provenance_notes,
};
use leptos::prelude::*;

const EMPTY: &str = "—";

/// The two counts a Timing run takes, clamped as they are typed so the number that runs is
/// the number on screen.
#[component]
pub fn TimingControls(
    playlist_requests: RwSignal<usize>,
    media_pairs: RwSignal<usize>,
) -> impl IntoView {
    view! {
        <div style="margin-top: calc(var(--spacing) * 2); padding-top: calc(var(--spacing) * 2); \
                    border-top: 1px dashed var(--color-sky-200);">
            <NumberField
                label="Playlist requests"
                hint="1–20"
                value=playlist_requests
                min=MIN_PLAYLIST_REQUESTS
                max=MAX_PLAYLIST_REQUESTS
            />
            <NumberField
                label="Media pairs"
                hint="1–10, video+audio together"
                value=media_pairs
                min=MIN_MEDIA_PAIRS
                max=MAX_MEDIA_PAIRS
            />
        </div>
    }
}

#[component]
fn NumberField(
    label: &'static str,
    hint: &'static str,
    value: RwSignal<usize>,
    min: usize,
    max: usize,
) -> impl IntoView {
    view! {
        <label style="display: flex; align-items: center; gap: calc(var(--spacing) * 1.5); \
                      font-size: .78rem; color: var(--color-sky-800); \
                      margin-bottom: var(--spacing);">
            <input
                type="number"
                min=min.to_string()
                max=max.to_string()
                style="width: 4.2rem; background: var(--color-white); \
                       border: 1px solid var(--color-sky-200); border-radius: 4px; \
                       padding: 2px 4px; font-size: .78rem; color: var(--color-sky-950);"
                prop:value=move || value.get().to_string()
                on:change=move |ev| {
                    let typed = event_target_value(&ev).trim().parse::<usize>().unwrap_or(min);
                    value.set(clamp_count(typed, min, max));
                }
            />
            {label}
            <span style="font-size: .68rem; color: var(--color-sky-700); font-style: italic;">
                {format!("({hint})")}
            </span>
        </label>
    }
}

/// The progress line and Stop button shown while a run is in flight.
#[component]
pub fn TimingProgressBar(
    progress: Memo<Option<TimingProgress>>,
    #[prop(into)] on_stop: Callback<()>,
) -> impl IntoView {
    view! {
        {move || progress.get().map(|p| {
            let pct = if p.total == 0 { 0.0 } else { (p.done as f64 / p.total as f64) * 100.0 };
            view! {
                <div style="margin-bottom: calc(var(--spacing) * 4); \
                            padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); \
                            background: var(--color-sky-50); \
                            border: 1px solid var(--color-sky-200); border-radius: 8px;">
                    <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 3); \
                                flex-wrap: wrap;">
                        <span style="font-size: .82rem; font-weight: 600; color: var(--color-sky-800);">
                            {"⏱ Timing"}
                        </span>
                        <span style="font-size: .8rem; color: var(--color-sky-700); flex: 1; min-width: 180px;">
                            {format!("{} — {} of {} request(s)", p.label, p.done, p.total)}
                        </span>
                        <button
                            type="button"
                            style="background: var(--color-white); color: var(--color-red-400); \
                                   border: 1px solid var(--color-red-400); border-radius: 6px; \
                                   padding: calc(var(--spacing) * 1.5) calc(var(--spacing) * 4); \
                                   font-size: .8rem; font-weight: 700; cursor: pointer;"
                            on:click=move |_| on_stop.run(())
                        >
                            "■ Stop"
                        </button>
                    </div>
                    <div style="height: 3px; background: var(--color-sky-200); border-radius: 2px; \
                                overflow: hidden; margin-top: calc(var(--spacing) * 2);">
                        <div style=format!(
                            "width: {pct:.0}%; height: 100%; border-radius: 2px; \
                             background: linear-gradient(90deg, var(--color-sky-300), var(--color-sky-500)); \
                             transition: width .2s;"
                        )></div>
                    </div>
                </div>
            }
        })}
    }
}

const TIMING_COLUMNS: &[&str] = &[
    "Request",
    "Rendition",
    "Sample",
    "URI",
    "Status",
    "Bytes",
    // Named at length on purpose: the app-observed header time is the figure most easily
    // mistaken for a TTFB, and it is not one.
    "App header time (not TTFB)",
    "True TTFB (needs TAO)",
    "App body time",
    "RT total (authoritative)",
    "App total",
    "Connection",
    "Media duration",
    "Target duration",
    "% of media duration",
    "% of target duration",
];

/// The Timing results: the caveats, the raw rows, and a median of them.
#[component]
pub fn TimingResultsPanel(report: TimingReport) -> impl IntoView {
    let declined = report.declined.clone();
    let notes: Vec<String> = report.notes.iter().map(|n| n.text.clone()).collect();
    let rows = report.rows.clone();
    let summary = medians(&report.rows);
    let total_bytes = report.total_bytes;
    let media_requests = report.rows.iter().filter(|r| r.class.is_media()).count();

    view! {
        <div style="margin-bottom: calc(var(--spacing) * 7);">
            <div style="font-size: 1rem; font-weight: 700; color: var(--color-sky-700); \
                        text-transform: uppercase; letter-spacing: .08em; \
                        margin-bottom: calc(var(--spacing) * 3); display: flex; \
                        align-items: center; gap: calc(var(--spacing) * 2);">
                "⏱ Timing"
                <span style="flex: 1; height: 1px; background: var(--color-sky-200);"></span>
            </div>

            {declined.map(|reason| view! {
                <div style="padding: calc(var(--spacing) * 3.5) calc(var(--spacing) * 4.5); \
                            background: rgba(239,68,68,.12); border: 1px solid var(--color-red-400); \
                            border-radius: 8px; color: var(--color-red-400); font-size: .85rem; \
                            margin-bottom: calc(var(--spacing) * 4);">
                    {format!("⚠ {reason}")}
                </div>
            })}

            {(!notes.is_empty()).then(|| view! {
                <div style="margin-bottom: calc(var(--spacing) * 3); \
                            padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); \
                            background: rgba(245,158,11,.1); border: 1px solid rgba(245,158,11,.35); \
                            border-radius: 8px; font-size: .8rem; color: #d97706;">
                    {notes.into_iter().map(|n| view! { <div>{format!("ⓘ {n}")}</div> })
                        .collect::<Vec<_>>()}
                </div>
            })}

            {(!rows.is_empty()).then(|| view! {
                <div>
                    <RawRowsTable rows=rows.clone() />
                    <MediansTable rows=summary.clone() />
                    <div style="font-size: .75rem; color: var(--color-sky-700); \
                                margin-bottom: calc(var(--spacing) * 3);">
                        {format!(
                            "{total_bytes} byte(s) downloaded by this run, across {media_requests} \
                             media request(s) and the playlist requests above."
                        )}
                    </div>
                </div>
            })}

            <div style="padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); \
                        background: var(--color-sky-50); border: 1px solid var(--color-sky-200); \
                        border-radius: 8px; font-size: .76rem; color: var(--color-sky-800);">
                <div style="font-weight: 700; text-transform: uppercase; letter-spacing: .05em; \
                            margin-bottom: calc(var(--spacing) * 1.5);">
                    "What these numbers are, and are not"
                </div>
                {provenance_notes().into_iter().map(|n| view! {
                    <div style="margin-bottom: calc(var(--spacing) * 0.75);">{format!("• {n}")}</div>
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

#[component]
fn RawRowsTable(rows: Vec<TimingRow>) -> impl IntoView {
    let cells: Vec<Vec<String>> = rows.iter().map(row_cells).collect();
    view! {
        <div style="overflow-x: auto; border: 1px solid var(--color-sky-200); border-radius: 10px; \
                    margin-bottom: calc(var(--spacing) * 4);">
            <table style="width: max-content; min-width: 100%; border-collapse: collapse; font-size: .8rem;">
                <thead>
                    <tr>
                        {TIMING_COLUMNS.iter().map(|h| view! {
                            <th style="text-align: left; padding: calc(var(--spacing) * 2.25) calc(var(--spacing) * 3); \
                                       background: var(--color-sky-100); \
                                       border-bottom: 2px solid var(--color-sky-200); \
                                       font-size: .68rem; font-weight: 700; color: var(--color-sky-700); \
                                       text-transform: uppercase; letter-spacing: .06em; white-space: nowrap;">
                                {*h}
                            </th>
                        }).collect::<Vec<_>>()}
                    </tr>
                </thead>
                <tbody>
                    {cells.into_iter().enumerate().map(|(i, row)| {
                        let bg = if i % 2 == 0 { "var(--color-white)" } else { "var(--color-sky-50)" };
                        view! {
                            <tr style=format!("background: {bg};")>
                                {row.into_iter().map(|value| view! {
                                    <td style="padding: calc(var(--spacing) * 1.75) calc(var(--spacing) * 3); \
                                               border-bottom: 1px solid var(--color-sky-100); \
                                               color: var(--color-sky-950); \
                                               font-family: ui-monospace, monospace; \
                                               white-space: nowrap; max-width: 26rem; overflow: hidden; \
                                               text-overflow: ellipsis;">
                                        {value}
                                    </td>
                                }).collect::<Vec<_>>()}
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn MediansTable(rows: Vec<MedianRow>) -> impl IntoView {
    view! {
        <div style="overflow-x: auto; border: 1px solid var(--color-sky-200); border-radius: 10px; \
                    margin-bottom: calc(var(--spacing) * 3);">
            <table style="width: max-content; min-width: 100%; border-collapse: collapse; font-size: .8rem;">
                <thead>
                    <tr>
                        {["Request", "Rendition", "Measured samples", "Median RT total", "Median app total"]
                            .into_iter().map(|h| view! {
                                <th style="text-align: left; padding: calc(var(--spacing) * 2.25) calc(var(--spacing) * 3); \
                                           background: var(--color-sky-100); \
                                           border-bottom: 2px solid var(--color-sky-200); \
                                           font-size: .68rem; font-weight: 700; color: var(--color-sky-700); \
                                           text-transform: uppercase; letter-spacing: .06em; white-space: nowrap;">
                                    {h}
                                </th>
                            }).collect::<Vec<_>>()}
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(|m| view! {
                        <tr>
                            {[
                                m.class.label().to_string(),
                                m.rendition.clone().unwrap_or_else(|| EMPTY.into()),
                                m.samples.to_string(),
                                fmt_ms(m.rt_total_median_ms),
                                fmt_ms(m.app_total_median_ms),
                            ].into_iter().map(|value| view! {
                                <td style="padding: calc(var(--spacing) * 1.75) calc(var(--spacing) * 3); \
                                           border-bottom: 1px solid var(--color-sky-100); \
                                           color: var(--color-sky-950); \
                                           font-family: ui-monospace, monospace; white-space: nowrap;">
                                    {value}
                                </td>
                            }).collect::<Vec<_>>()}
                        </tr>
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

/// One row's cells, in the order of [`TIMING_COLUMNS`].
///
/// A row that failed or was cancelled shows why in the status column and an em dash
/// everywhere a duration would go.
fn row_cells(row: &TimingRow) -> Vec<String> {
    vec![
        row.class.label().to_string(),
        row.rendition.clone().unwrap_or_else(|| EMPTY.into()),
        row.sample_label.clone(),
        uri_cell(row),
        status_cell(row),
        bytes_cell(row),
        fmt_ms(row.app_header_ms()),
        fmt_ms(row.true_ttfb_ms()),
        fmt_ms(row.app_body_ms()),
        fmt_ms(row.rt_total_ms()),
        fmt_ms(row.app_total_ms()),
        row.connection_state().label().to_string(),
        media_duration_cell(row),
        target_duration_cell(row),
        percent_cell(row.percent_of_media_duration(), row),
        percent_cell(row.percent_of_target_duration(), row),
    ]
}

fn status_cell(row: &TimingRow) -> String {
    match (&row.outcome, row.status()) {
        (_, Some(status)) if row.range_ignored() => format!("{status} (range ignored)"),
        (_, Some(status)) => status.to_string(),
        (RowOutcome::Failed(reason), _) => format!("failed: {reason}"),
        (RowOutcome::Cancelled, _) => "cancelled".into(),
        (RowOutcome::Skipped(reason), _) => format!("skipped: {reason}"),
        (RowOutcome::Measured(_), None) => EMPTY.into(),
    }
}

/// The URI asked for, the range asked of it, and where a redirect actually sent the
/// request. A redirect's cost is inside the measured time, so the row has to show it.
fn uri_cell(row: &TimingRow) -> String {
    let mut cell = match row.byterange {
        Some(range) => format!("{} [{range}]", row.uri),
        None => row.uri.clone(),
    };
    if let Some(final_url) = row.redirected_to() {
        cell.push_str(&format!(" → {final_url}"));
    }
    cell
}

/// Bytes as the app counted them, and what crossed the wire where that is known and
/// different. Only one of the two is a network quantity, so the source is named.
fn bytes_cell(row: &TimingRow) -> String {
    let Some(bytes) = row.bytes() else {
        return EMPTY.into();
    };
    match row.wire_bytes() {
        Some((wire, source)) if wire != bytes => format!("{bytes} ({wire} {source})"),
        _ => bytes.to_string(),
    }
}

/// What this request actually carries: EXTINF, or a part's DURATION. The primary
/// denominator, because it describes this sample rather than bounding it.
fn media_duration_cell(row: &TimingRow) -> String {
    match (row.media_duration_s, row.media_duration_source) {
        (Some(d), Some(source)) => format!("{d:.3} s ({})", source.label()),
        (Some(d), None) => format!("{d:.3} s"),
        _ => EMPTY.into(),
    }
}

/// The playlist's upper bound on any segment or part. Secondary: it is not this sample's
/// length, and a percentage against it says less than one against EXTINF.
fn target_duration_cell(row: &TimingRow) -> String {
    match (row.target_duration_s, row.target_duration_source) {
        (Some(t), Some(source)) => format!("≤ {t:.3} s ({})", source.label()),
        (Some(t), None) => format!("≤ {t:.3} s"),
        _ => EMPTY.into(),
    }
}

/// A percentage, with the clock it was measured against named. The source matters: a
/// figure divided by an app-observed total includes JS work the network never did.
///
/// An ignored range has no percentage to show: the duration covers the whole resource
/// while the denominator covers the slice that was asked for, so the cell says why it is
/// empty rather than printing a ratio between two different things.
fn percent_cell(percent: Option<f64>, row: &TimingRow) -> String {
    if row.range_ignored() {
        return format!("{EMPTY} (range ignored)");
    }
    let Some(percent) = percent else {
        return EMPTY.into();
    };
    let Some((_, source)) = row.authoritative_total_ms() else {
        return EMPTY.into();
    };
    match source {
        TotalSource::ResourceTiming => format!("{percent:.1}%"),
        // Named, because an app-observed total includes work the network never did.
        other => format!("{percent:.1}% ({})", other.label()),
    }
}

/// A duration, or an em dash. Never a zero standing in for something the browser did not
/// expose.
fn fmt_ms(value: Option<f64>) -> String {
    match value {
        Some(ms) => format!("{ms:.1} ms"),
        None => EMPTY.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::timing::{
        AppClockSpan, Measurement, RequestClass, ResourceTimingSample, RowOutcome,
    };
    use pretty_assertions::assert_eq;

    fn row(outcome: RowOutcome) -> TimingRow {
        TimingRow {
            class: RequestClass::VideoSegment,
            rendition: Some("720p".into()),
            sample_label: "pair 1 / 4".into(),
            uri: "https://example.com/1.m4s".into(),
            byterange: None,
            media_duration_s: Some(4.0),
            media_duration_source: None,
            target_duration_s: Some(6.0),
            target_duration_source: None,
            outcome,
        }
    }

    fn measured(rt: Option<ResourceTimingSample>) -> RowOutcome {
        RowOutcome::Measured(Measurement {
            status: 200,
            bytes: 1_000,
            declared_length: None,
            redirected_to: None,
            app: AppClockSpan {
                request_ms: 0.0,
                headers_ms: 40.0,
                body_end_ms: 200.0,
            },
            rt,
            range_ignored: false,
        })
    }

    #[test]
    fn every_row_fills_every_column() {
        for outcome in [
            measured(None),
            RowOutcome::Failed("NetworkError".into()),
            RowOutcome::Cancelled,
            RowOutcome::Skipped("byte cap".into()),
        ] {
            assert_eq!(row_cells(&row(outcome)).len(), TIMING_COLUMNS.len());
        }
    }

    #[test]
    fn a_missing_measurement_renders_as_an_em_dash_not_a_zero() {
        let cells = row_cells(&row(RowOutcome::Cancelled));
        assert!(cells.contains(&"cancelled".to_string()));
        // Bytes, five duration columns and both percentages.
        assert_eq!(cells.iter().filter(|c| *c == EMPTY).count(), 8);
        // Nothing that reads as a measurement survives a cancelled request.
        assert!(!cells.iter().any(|c| c.contains(" ms")));
        assert!(!cells.iter().any(|c| c.contains('%')));
    }

    #[test]
    fn a_percentage_taken_from_the_app_clock_says_so() {
        let without_rt = row_cells(&row(measured(None)));
        assert!(
            without_rt
                .iter()
                .any(|c| c == "5.0% (app-observed)"),
            "{without_rt:?}"
        );
        let with_rt = row_cells(&row(measured(Some(ResourceTimingSample {
            start_time: 0.0,
            response_end: 120.0,
            ..Default::default()
        }))));
        assert!(with_rt.iter().any(|c| c == "3.0%"), "{with_rt:?}");
    }

    #[test]
    fn a_range_the_server_ignored_is_visible_in_the_status_cell() {
        let mut r = row(measured(None));
        r.byterange = Some(crate::utils::network::RequestRange::from_length_with_offset(
            1_000, 500,
        ));
        if let RowOutcome::Measured(m) = &mut r.outcome {
            m.range_ignored = true;
        }
        let cells = row_cells(&r);
        assert!(cells.iter().any(|c| c == "200 (range ignored)"));
        assert!(cells.iter().any(|c| c.contains("[500-1499]")));
    }

    #[test]
    fn an_ignored_range_shows_no_percentage_and_says_why() {
        let mut r = row(measured(Some(ResourceTimingSample {
            start_time: 0.0,
            response_end: 120.0,
            ..Default::default()
        })));
        r.byterange = Some(crate::utils::network::RequestRange::from_length_with_offset(
            1_000, 500,
        ));
        if let RowOutcome::Measured(m) = &mut r.outcome {
            m.range_ignored = true;
        }
        let cells = row_cells(&r);
        // A ratio of the whole resource's duration to one slice's EXTINF is not a ratio.
        assert!(!cells.iter().any(|c| c.contains('%')), "{cells:?}");
        assert_eq!(
            cells
                .iter()
                .filter(|c| *c == &format!("{EMPTY} (range ignored)"))
                .count(),
            2
        );
        // The row still says the range was ignored where the status is reported.
        assert!(cells.iter().any(|c| c == "200 (range ignored)"));
    }

    #[test]
    fn durations_are_formatted_or_suppressed() {
        assert_eq!(fmt_ms(Some(12.34)), "12.3 ms");
        assert_eq!(fmt_ms(Some(0.0)), "0.0 ms");
        assert_eq!(fmt_ms(None), EMPTY);
    }
}
