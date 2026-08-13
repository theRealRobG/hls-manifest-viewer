use crate::utils::{
    href::{replace_hls_variables, DEFINITIONS_QUERY_NAME, SUPPLEMENTAL_VIEW_QUERY_NAME},
    query_codec::{encode_asset_list, encode_definitions},
    validator::{self, types::*},
};
use leptos::prelude::*;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

// ── Constants ────────────────────────────────────────────────────────────────


// ── Helper functions ─────────────────────────────────────────────────────────

fn compress_codecs(codecs: &str) -> String {
    if codecs.is_empty() {
        return "—".to_string();
    }
    let mapped: Vec<&str> = codecs.split(',').map(|p| {
        let p = p.trim();
        let pl = p.to_lowercase();
        if pl.starts_with("hvc1") || pl.starts_with("hev1") { "HEVC" }
        else if pl.starts_with("avc1") || pl.starts_with("avc3") { "H.264" }
        else if pl.starts_with("mp4a.40.2") { "AAC" }
        else if pl.starts_with("mp4a.40.5") { "HE-AAC" }
        else if pl.starts_with("ec-3") || pl.starts_with("ec3") { "EC-3" }
        else if pl.starts_with("ac-4") || pl.starts_with("ac4") { "AC-4" }
        else if pl.starts_with("opus") { "Opus" }
        else if pl.starts_with("vp09") || pl.starts_with("vp9") { "VP9" }
        else if pl.starts_with("av01") { "AV1" }
        else if pl.starts_with("dvh1") || pl.starts_with("dvhe") { "HEVC" }
        else if pl.starts_with("dav1") || pl.starts_with("dva1") || pl.starts_with("dvav") { "AV1" }
        else { p }
    }).collect();
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<&str> = mapped.into_iter().filter(|s| seen.insert(*s)).collect();
    unique.join(" + ")
}

fn format_bandwidth(bw: u64) -> String {
    if bw == 0 { return "—".to_string(); }
    if bw >= 1_000_000 {
        format!("{:.2} Mbps", bw as f64 / 1_000_000.0)
    } else {
        format!("{} kbps", bw / 1000)
    }
}

fn fmt_time(secs: f64) -> String {
    let s = secs as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, sec)
    } else {
        format!("{}:{:02}", m, sec)
    }
}

fn colour_badge_html(color_info: &str) -> (String, String, String) {
    // Returns (bg, color, border)
    if color_info.starts_with("Dolby Vision") {
        ("rgba(168,85,247,.15)".into(), "#c084fc".into(), "rgba(168,85,247,.3)".into())
    } else if color_info == "HDR10" {
        ("rgba(245,158,11,.15)".into(), "#f59e0b".into(), "rgba(245,158,11,.35)".into())
    } else if color_info == "HLG" {
        ("rgba(20,184,166,.15)".into(), "#14b8a6".into(), "rgba(20,184,166,.35)".into())
    } else {
        // SDR or unknown
        ("rgba(148,163,184,.15)".into(), "#64748b".into(), "rgba(148,163,184,.3)".into())
    }
}

/// Definitions to pass into the manifest viewer for a validation report.
/// Prefer the master playlist's EXT-X-DEFINE map; fall back to the first media playlist
/// when the input was a media-only URL.
fn report_definitions(report: &ValidationReport) -> std::collections::HashMap<String, String> {
    if let Some(master) = &report.master {
        master.definitions.clone()
    } else {
        report
            .playlists
            .first()
            .map(|p| p.definitions.clone())
            .unwrap_or_default()
    }
}

/// True for duration-drift issues that should link to the compared renditions in the viewer.
fn is_drift_issue_message(message: &str) -> bool {
    message.starts_with("EXTINF drift") || message.starts_with("Cumulative EXTINF drift")
}

/// Build an absolute manifest-viewer URL for `playlist_url`, applying HLS variable substitution
/// and appending `imported_definitions` when the definitions map is non-empty.
fn manifest_viewer_href(
    url: &str,
    definitions: &std::collections::HashMap<String, String>,
) -> String {
    let resolved = replace_hls_variables(url, definitions);
    let encoded_url = utf8_percent_encode(&resolved, NON_ALPHANUMERIC);
    if definitions.is_empty() {
        format!("/hls-manifest-viewer/?playlist_url={encoded_url}")
    } else {
        let encoded_defs = encode_definitions(definitions);
        format!(
            "/hls-manifest-viewer/?playlist_url={encoded_url}&{DEFINITIONS_QUERY_NAME}={encoded_defs}"
        )
    }
}

/// Build a manifest-viewer URL where `rendition_url` is the primary playlist and
/// the asset list is shown in the supplemental view panel.  This avoids making
/// the JSON endpoint the main playlist URL (which causes the viewer to break when
/// the ad proxy returns a 500 outside of a live session).
fn manifest_viewer_href_with_asset_list(
    rendition_url: &str,
    asset_list_url: &str,
    daterange_id: &str,
    definitions: &std::collections::HashMap<String, String>,
) -> String {
    let resolved_rendition = replace_hls_variables(rendition_url, definitions);
    let resolved_asset_list = replace_hls_variables(asset_list_url, definitions);
    let encoded_rendition = utf8_percent_encode(&resolved_rendition, NON_ALPHANUMERIC);
    let encoded_supplemental = encode_asset_list(&resolved_asset_list, daterange_id);
    if definitions.is_empty() {
        format!(
            "/hls-manifest-viewer/?playlist_url={encoded_rendition}&{SUPPLEMENTAL_VIEW_QUERY_NAME}={encoded_supplemental}"
        )
    } else {
        let encoded_defs = encode_definitions(definitions);
        format!(
            "/hls-manifest-viewer/?playlist_url={encoded_rendition}&{DEFINITIONS_QUERY_NAME}={encoded_defs}&{SUPPLEMENTAL_VIEW_QUERY_NAME}={encoded_supplemental}"
        )
    }
}

// ── Main component ───────────────────────────────────────────────────────────

#[component]
pub fn Validate() -> impl IntoView {
    let (url_input, set_url_input) = signal(String::new());
    let (tolerance, set_tolerance) = signal(100.0_f64);
    let (report, set_report) = signal(None::<ValidationReport>);
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let url = url_input.get();
        if url.is_empty() { return; }
        let tol = tolerance.get();
        set_loading.set(true);
        set_error_msg.set(None);
        set_report.set(None);
        leptos::task::spawn_local(async move {
            match validator::validate_hls_with_options(&url, tol).await {
                Ok(r) => set_report.set(Some(r)),
                Err(e) => set_error_msg.set(Some(format!("Validation failed: {}", e))),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="body-content" style="margin-bottom: 2em;">
            <h1 class="body-content">
                "Validate HLS streams instantly"
            </h1>
            <p class="body-content body-text">
                "Enter a master or media playlist URL to run 20+ compliance checks against RFC 8216 and the HLS bis draft — structural integrity, alignment, LL-HLS, encryption and more. Media-only URLs run a reduced set of checks on that playlist alone."
            </p>
            <div style="background: var(--color-white); border: 1px solid var(--color-sky-200); border-radius: 12px; padding: calc(var(--spacing) * 7); box-shadow: 0 2px 12px rgba(0,0,0,.06); margin-top: calc(var(--spacing) * 6);">
                <form on:submit=on_submit>
                    <div style="display: flex; gap: calc(var(--spacing) * 2.5); flex-wrap: wrap;">
                        <input
                            type="url"
                            placeholder="https://example.com/stream/master.m3u8"
                            style="flex: 1; min-width: 260px; background: var(--color-sky-50); border: 1.5px solid var(--color-sky-200); border-radius: 8px; color: var(--color-sky-950); font-size: 1rem; padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); outline: none;"
                            prop:value=move || url_input.get()
                            on:input=move |ev| set_url_input.set(event_target_value(&ev))
                        />
                        <button
                            type="submit"
                            style="background: linear-gradient(135deg, var(--color-sky-300), var(--color-sky-500)); color: var(--color-white); border: none; border-radius: 8px; padding: calc(var(--spacing) * 3) calc(var(--spacing) * 7); font-size: 1rem; font-weight: 700; cursor: pointer; white-space: nowrap;"
                            disabled=move || loading.get()
                        >
                            {move || if loading.get() { "⏳ Validating..." } else { "▶ Validate" }}
                        </button>
                    </div>
                    // Options row
                    <div style="display: flex; flex-wrap: wrap; gap: calc(var(--spacing) * 5); margin-top: calc(var(--spacing) * 4.5); align-items: center;">
                        <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 2); font-size: .875rem; color: var(--color-sky-700);">
                            <label>"Tolerance"</label>
                            <input type="number"
                                style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 6px; color: var(--color-sky-950); padding: calc(var(--spacing) * 1.25) calc(var(--spacing) * 2.5); width: 80px; font-size: .875rem; outline: none;"
                                prop:value=move || tolerance.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                        set_tolerance.set(v);
                                    }
                                }
                            />
                            <label>"ms"</label>
                        </div>
                        // Inspect manifest link (shown after validation)
                        {move || report.get().map(|r| {
                            let defs = report_definitions(&r);
                            let viewer_url = manifest_viewer_href(&r.master_url, &defs);
                            view! {
                                <a href=viewer_url target="_blank" rel="noopener noreferrer"
                                    style="display: inline-flex; align-items: center; gap: calc(var(--spacing) * 1.5); font-size: .82rem; font-weight: 700; color: var(--color-white); background: linear-gradient(135deg, var(--color-sky-300), var(--color-sky-500)); border-radius: 6px; padding: calc(var(--spacing) * 1.75) calc(var(--spacing) * 3.5); text-decoration: none; white-space: nowrap; margin-left: auto;">
                                    "🔗 Inspect the manifest"
                                </a>
                            }
                        })}
                    </div>
                </form>
                {move || loading.get().then(|| view! {
                    <div style="margin-top: calc(var(--spacing) * 3);">
                        <div style="height: 3px; background: var(--color-sky-200); border-radius: 2px; overflow: hidden;">
                            <div style="width: 40%; height: 100%; background: linear-gradient(90deg, var(--color-sky-300), var(--color-sky-500)); animation: progress 1.5s ease-in-out infinite; border-radius: 2px;"></div>
                        </div>
                        <div style="font-size: .85rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 1.5);">"Fetching playlists and running checks…"</div>
                    </div>
                })}
                {move || error_msg.get().map(|e| view! {
                    <div style="margin-top: calc(var(--spacing) * 3); padding: calc(var(--spacing) * 3.5) calc(var(--spacing) * 4.5); background: rgba(239,68,68,.15); border: 1px solid var(--color-red-400); border-radius: 8px; color: var(--color-red-400); font-size: .9rem;">
                        {format!("⚠ {}", e)}
                    </div>
                })}
            </div>
        </div>

        {move || report.get().map(|r| view! { <ValidationResults report=r /> })}
    }
}

// ── Results container ────────────────────────────────────────────────────────

#[component]
fn ValidationResults(report: ValidationReport) -> impl IntoView {
    let is_pass = report.result == "PASS";
    let errors = report.total_errors;
    let warnings = report.total_warnings;
    let renditions = report.renditions.clone();
    let interstitials = report.interstitials.clone();
    let check_groups = report.check_groups.clone();
    let delta_report = report.delta_report.clone();
    let ad_breaks = report.ad_breaks.clone();
    let elapsed = report.elapsed_ms;
    let tolerance = report.tolerance_ms;
    let has_interstitials_data = report.has_interstitials_data;
    let has_scte35_data = report.has_scte35_data;

    let playlist_window_s = report.playlist_window_s;
    let latency_info = compute_latency(&renditions);
    let definitions = report_definitions(&report);
    let rend_count = renditions.len();
    let error_color: &'static str = if errors > 0 { "#ef4444" } else { "#22c55e" };
    let warn_color: &'static str = if warnings > 0 { "#f59e0b" } else { "#22c55e" };
    let has_renditions = !renditions.is_empty();
    let has_interstitials = !interstitials.is_empty();
    let has_deltas = !delta_report.is_empty();
    let tol_str = format!("{}", tolerance as u64);

    view! {
        <div class="body-content" style="max-width: 80rem;">
            <ResultBanner is_pass=is_pass errors=errors warnings=warnings
                rendition_count=rend_count elapsed_ms=elapsed />

            // Stats row — flex so all cards stay on one line
            <div style="display: flex; flex-wrap: nowrap; gap: calc(var(--spacing) * 3); margin-bottom: calc(var(--spacing) * 7);">
                <StatCard value=rend_count.to_string() label="Renditions" color="#60a5fa" />
                <StatCard value=errors.to_string() label="Errors" color=error_color />
                <StatCard value=warnings.to_string() label="Warnings" color=warn_color />
                <StatCard value=tol_str label="Tolerance (ms)" color="#60a5fa" />
                {latency_info.map(|(val, lbl)| {
                    let lbl_static: &'static str = Box::leak(lbl.into_boxed_str());
                    view! {
                        <StatCard value=val label=lbl_static color="#60a5fa" />
                    }
                })}
            </div>

            // Section: Renditions
            {has_renditions.then(|| view! {
                <SectionTitle label="📼 Renditions" />
                <RenditionsTable renditions=renditions.clone() definitions=definitions.clone() />
            })}

            // Section: Interstitials
            {has_interstitials.then(|| view! {
                <SectionTitle label="📌 HLS Interstitials" />
                <InterstitialTimeline interstitials=interstitials.clone() renditions=renditions.clone() />
                <InterstitialsSection interstitials=interstitials.clone() />
            })}

            // Section: SCTE-35 Ad Breaks
            {has_scte35_data.then(|| view! {
                <SectionTitle label="📡 SCTE-35 Ad Breaks" />
                <Scte35Section
                    ad_breaks=ad_breaks.clone()
                    playlist_window_s=playlist_window_s
                    definitions=definitions.clone()
                />
            })}

            // Section: Delta Updates
            {has_deltas.then(|| view! {
                <SectionTitle label="⏩ Playlist Delta Updates" />
                <DeltaSection deltas=delta_report.clone() definitions=definitions.clone() />
            })}
            {(!has_deltas).then(|| view! {
                <SectionTitle label="⏩ Playlist Delta Updates" />
                <div style="color: var(--color-sky-700); font-size: .88rem; margin-bottom: calc(var(--spacing) * 7); padding: calc(var(--spacing) * 3.5) calc(var(--spacing) * 4.5); background: var(--color-sky-50); border-radius: 8px; border: 1px solid var(--color-sky-200);">
                    "No renditions advertise CAN-SKIP-UNTIL — Playlist Delta Updates are not offered by this stream."
                </div>
            })}

            // Section: Check Results
            <SectionTitle label="🔍 Check Results" />
            <CheckResultsTable
                groups=check_groups
                has_interstitials_data=has_interstitials_data
                renditions=renditions.clone()
                definitions=definitions
            />

            <div style="text-align: center; padding: calc(var(--spacing) * 8) calc(var(--spacing) * 5) 0; font-size: .8rem; color: var(--color-sky-700);">
                "HLS Validator · Based on "
                <a href="https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis" target="_blank" style="color: var(--color-sky-500);">
                    "draft-pantos-hls-rfc8216bis"
                </a>
                " & RFC 8216"
            </div>
        </div>
    }
}

fn compute_latency(renditions: &[Rendition]) -> Option<(String, String)> {
    let primary = renditions.iter().find(|r| r.media_type == "VIDEO")
        .or_else(|| renditions.first())?;
    if primary.has_parts
        && let Some(phb) = primary.part_hold_back
            && phb > 0.0 {
                let disp = if phb >= 1.0 { format!("{:.1}s", phb) } else { format!("{}ms", (phb * 1000.0) as u64) };
                return Some((disp, "Est. Latency · LL-HLS".to_string()));
            }
    let hb = primary.hold_back.unwrap_or(0.0);
    let latency = if hb > 0.0 { hb } else { primary.target_duration * 3.0 };
    if latency <= 0.0 { return None; }
    let disp = if latency >= 1.0 { format!("{:.1}s", latency) } else { format!("{}ms", (latency * 1000.0) as u64) };
    Some((disp, "Est. Latency · HLS".to_string()))
}

// ── PASS/FAIL Banner ─────────────────────────────────────────────────────────

#[component]
fn ResultBanner(is_pass: bool, errors: usize, warnings: usize, rendition_count: usize, elapsed_ms: u64) -> impl IntoView {
    let (bg, border_color) = if is_pass {
        ("rgba(34,197,94,.12)", "#22c55e")
    } else {
        ("rgba(239,68,68,.12)", "#ef4444")
    };
    let icon = if is_pass { "✅" } else { "❌" };
    let label = if is_pass { "PASS" } else { "FAIL" };
    let title_color = if is_pass { "#22c55e" } else { "#ef4444" };

    view! {
        <div style=format!(
            "display: flex; align-items: center; gap: 20px; padding: 24px 28px; \
             background: {}; border: 1px solid {}; border-radius: 12px; margin-bottom: 20px;",
            bg, border_color
        )>
            <div style="font-size: 2.5rem;">{icon}</div>
            <div>
                <div style=format!("font-size: 1.6rem; font-weight: 800; color: {};", title_color)>
                    {label}
                </div>
                <div style="font-size: .9rem; color: var(--color-sky-700);">
                    {format!("{} rendition{} analysed · {} error{} · {} warning{}",
                        rendition_count, if rendition_count != 1 { "s" } else { "" },
                        errors, if errors != 1 { "s" } else { "" },
                        warnings, if warnings != 1 { "s" } else { "" }
                    )}
                </div>
            </div>
            <div style="margin-left: auto; text-align: right; font-size: .8rem; color: var(--color-sky-700);">
                {format!("Completed in {}ms", elapsed_ms)}
            </div>
        </div>
    }
}

// ── Section Title ────────────────────────────────────────────────────────────

#[component]
fn SectionTitle(label: &'static str) -> impl IntoView {
    view! {
        <div style="font-size: 1rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .08em; margin-bottom: calc(var(--spacing) * 3); display: flex; align-items: center; gap: calc(var(--spacing) * 2);">
            {label}
            <span style="flex: 1; height: 1px; background: var(--color-sky-200);"></span>
        </div>
    }
}

// ── Stat Card ────────────────────────────────────────────────────────────────

#[component]
fn StatCard(value: String, label: &'static str, color: &'static str) -> impl IntoView {
    view! {
        <div style=format!(
            "flex: 1; min-width: 100px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 5); background: var(--color-sky-50); \
             border: 1px solid var(--color-sky-200); border-radius: 12px; text-align: center;"
        )>
            <div style=format!("font-size: 1.5rem; font-weight: 700; color: {};", color)>
                {value}
            </div>
            <div style="font-size: .8rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 0.5);">{label}</div>
        </div>
    }
}

// ── Renditions Table ─────────────────────────────────────────────────────────

#[component]
fn RenditionsTable(
    renditions: Vec<Rendition>,
    definitions: std::collections::HashMap<String, String>,
) -> impl IntoView {
    let mut sorted = renditions;
    sorted.sort_by(|a, b| {
        if a.media_type != b.media_type {
            return if a.media_type == "AUDIO" { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less };
        }
        a.bandwidth.cmp(&b.bandwidth)
    });

    let th_style = "padding: var(--spacing) calc(var(--spacing) * 3) calc(var(--spacing) * 2); font-size: .72rem; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .07em; text-align: left; font-weight: 600;";

    view! {
        <div style="overflow-x: auto; margin-bottom: calc(var(--spacing) * 7);">
            <table style="width: 100%; border-collapse: separate; border-spacing: 0 4px;">
                <thead>
                    <tr>
                        <th style=th_style>"Type"</th>
                        <th style=th_style>"Name / Group"</th>
                        <th style=th_style>"Resolution"</th>
                        <th style=th_style>"Bandwidth"</th>
                        <th style=th_style>"Avg Bandwidth"</th>
                        <th style=th_style>"Frame Rate"</th>
                        <th style=th_style>"Colour"</th>
                        <th style=th_style>"Codecs"</th>
                        <th style=th_style>"Closed Captions"</th>
                        <th style=th_style>"Segments"</th>
                    </tr>
                </thead>
                <tbody>
                    {sorted.into_iter().map(|rn| {
                        let is_audio = rn.media_type == "AUDIO";
                        let type_badge_style = if is_audio {
                            "background: rgba(168,85,247,.15); color: #c084fc; border: 1px solid rgba(168,85,247,.3); border-radius: 6px; padding: 3px 9px; font-size: .72rem; font-weight: 700; white-space: nowrap;"
                        } else {
                            "background: rgba(56,189,248,.15); color: #38bdf8; border: 1px solid rgba(56,189,248,.3); border-radius: 6px; padding: 3px 9px; font-size: .72rem; font-weight: 700; white-space: nowrap;"
                        };
                        let type_label = if is_audio { "AUDIO" } else { "VIDEO" };
                        let resolution = if is_audio { "—".to_string() } else { rn.resolution.clone().unwrap_or_else(|| "—".to_string()) };
                        let bw = format_bandwidth(rn.bandwidth);
                        let avg_bw = rn.average_bandwidth.map(format_bandwidth).unwrap_or_else(|| "—".to_string());
                        let frame_rate = if is_audio { "—".to_string() } else {
                            rn.frame_rate.map(|f| format!("{:.2} fps", f)).unwrap_or_else(|| "—".to_string())
                        };
                        let codecs = rn.codecs.as_deref().map(compress_codecs).unwrap_or_else(|| "—".to_string());
                        let group_label = if is_audio { "Group" } else { "Audio ref" };
                        let group_id = rn.group_id.clone().unwrap_or_default();
                        let cc = if is_audio { "—".to_string() } else { rn.closed_captions.clone().unwrap_or_else(|| "—".to_string()) };
                        let viewer_url = manifest_viewer_href(&rn.url, &definitions);

                        // Colour badge
                        let colour_view = if is_audio {
                            view! { <span>"—"</span> }.into_any()
                        } else if let Some(ref ci) = rn.color_info {
                            let (bg, color, border) = colour_badge_html(ci);
                            let style = format!(
                                "display: inline-block; border-radius: 5px; padding: 2px 8px; \
                                 font-size: .7rem; font-weight: 700; white-space: nowrap; \
                                 background: {}; color: {}; border: 1px solid {};",
                                bg, color, border
                            );
                            view! { <span style=style>{ci.clone()}</span> }.into_any()
                        } else {
                            view! { <span>"—"</span> }.into_any()
                        };

                        let td_style = "padding: calc(var(--spacing) * 2.5) calc(var(--spacing) * 3); vertical-align: middle; background: var(--color-sky-50);";

                        view! {
                            <tr style="transition: background .15s;">
                                <td style=td_style>
                                    <span style=type_badge_style>{type_label}</span>
                                </td>
                                <td style=td_style>
                                    <div style="font-weight: 600; font-size: .9rem; word-break: break-all;">
                                        <a href=viewer_url target="_blank" rel="noopener noreferrer"
                                           style="color: var(--color-sky-500); font-size: .8rem; font-weight: 600; display: inline-flex; align-items: center; gap: calc(var(--spacing) * 1.25); border: 1px solid rgba(56,189,248,.3); border-radius: 5px; padding: var(--spacing) calc(var(--spacing) * 2.5); background: rgba(56,189,248,.06); text-decoration: none;">
                                            {rn.name.clone()}
                                        </a>
                                    </div>
                                    {(!group_id.is_empty()).then(|| view! {
                                        <div style="font-size: .75rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 0.5);">
                                            {format!("{}: {}", group_label, group_id)}
                                        </div>
                                    })}
                                </td>
                                <td style=td_style>{resolution}</td>
                                <td style=format!("{} white-space: nowrap;", td_style)>{bw}</td>
                                <td style=format!("{} white-space: nowrap;", td_style)>{avg_bw}</td>
                                <td style=td_style>{frame_rate}</td>
                                <td style=td_style>{colour_view}</td>
                                <td style=td_style>{codecs}</td>
                                <td style=td_style>{cc}</td>
                                <td style=td_style>{rn.segment_count}</td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

// ── Interstitial Timeline ────────────────────────────────────────────────────

#[component]
fn InterstitialTimeline(interstitials: Vec<Interstitial>, renditions: Vec<Rendition>) -> impl IntoView {
    let unique = dedup_interstitials(interstitials);
    if unique.is_empty() {
        return view! { <div></div> }.into_any();
    }

    // Calculate total duration
    let mut total_dur: f64 = unique.iter().map(|it| it.content_duration_s).fold(0.0_f64, f64::max);
    if total_dur <= 0.0 {
        let best = renditions.iter()
            .filter(|r| r.media_type != "AUDIO")
            .max_by_key(|r| r.bandwidth);
        if let Some(b) = best {
            total_dur = b.segment_count as f64 * b.target_duration.max(6.0);
        }
    }
    let has_offsets = unique.iter().any(|it| it.start_offset_s.map(|o| o >= 0.0).unwrap_or(false));
    // Ensure total covers last marker + playout (prefer X-PLAYOUT-LIMIT, fall back to PLANNED-DURATION)
    for it in &unique {
        let off = it.start_offset_s.unwrap_or(0.0);
        let dur = it.playout_limit.or(it.planned_duration_s).unwrap_or(0.0);
        let end = off + dur;
        if end > total_dur { total_dur = end + 30.0; }
    }
    if total_dur <= 0.0 { total_dur = 3600.0; }

    // Time axis ticks
    let intervals = [15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0];
    let interval = intervals.iter().find(|&&i| total_dur / i <= 6.0).copied().unwrap_or(3600.0);
    let mut ticks = Vec::new();
    let mut t = 0.0;
    while t <= total_dur {
        ticks.push((t / total_dur * 100.0, fmt_time(t)));
        t += interval;
    }

    let unique_len = unique.len();
    let total_dur_fmt = fmt_time(total_dur);
    let note = if has_offsets { "" } else { " (relative timing)" };

    view! {
        <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4.5) calc(var(--spacing) * 5); margin-bottom: calc(var(--spacing) * 4);">
            <div style="font-size: .78rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .06em; margin-bottom: calc(var(--spacing) * 3);">
                {format!("🎬 Interstitial Timeline — {} event{} · {} total{}", unique_len, if unique_len != 1 { "s" } else { "" }, total_dur_fmt, note)}
            </div>
            // Above-track markers — clickable <a> tags opening primary playlist in manifest viewer
            <div style="position: relative; height: 28px; margin-bottom: 4px;">
                {unique.iter().map(|it| {
                    let off = it.start_offset_s.unwrap_or(0.0);
                    let left_pct = (off / total_dur * 100.0).min(99.5);
                    let has_err = !it.errors.is_empty();
                    let bg = if has_err { "rgba(239,68,68,.55)" } else { "rgba(168,85,247,.55)" };
                    let border = if has_err { "rgba(239,68,68,.8)" } else { "rgba(168,85,247,.8)" };
                    // Use X-PLAYOUT-LIMIT for width; fall back to PLANNED-DURATION when absent
                    let eff_dur = it.playout_limit.or(it.planned_duration_s).unwrap_or(0.0);
                    let width_pct = if eff_dur > 0.0 { (eff_dur / total_dur * 100.0).max(0.4) } else { 0.4 };
                    let marker_style = format!(
                        "position: absolute; top: 0; left: {:.2}%; width: {:.2}%; height: 28px; \
                         border-radius: 4px; background: {}; border: 1px solid {}; cursor: pointer; \
                         z-index: 2; display: block; text-decoration: none; overflow: hidden;",
                        left_pct, width_pct, bg, border
                    );
                    let label = if width_pct >= 1.5 { it.id.clone() } else { String::new() };
                    let tooltip = format!("{} @ {}", it.id, fmt_time(off));
                    // Link to primary media playlist in manifest viewer (with variable definitions)
                    let viewer_href = manifest_viewer_href(&it.rendition_url, &it.definitions);
                    view! {
                        <a href=viewer_href target="_blank" rel="noopener noreferrer"
                           style=marker_style title=tooltip>
                            {(!label.is_empty()).then(|| view! {
                                <div style="position: absolute; top: 50%; left: 50%; transform: translate(-50%,-50%); font-size: .6rem; font-weight: 700; white-space: nowrap; color: #fff; text-shadow: 0 1px 2px rgba(0,0,0,.6); pointer-events: none; overflow: hidden; max-width: 90%;">
                                    {label}
                                </div>
                            })}
                        </a>
                    }
                }).collect::<Vec<_>>()}
            </div>
            // Track bar with resume-offset spans
            <div style="position: relative; height: 28px; border-radius: 6px; overflow: visible; background: rgba(148,163,184,.06); border: 1px solid var(--color-sky-200);">
                {unique.iter().filter_map(|it| {
                    let off = it.start_offset_s.unwrap_or(0.0);
                    let left_pct = (off / total_dur * 100.0).min(99.5);
                    it.resume_offset.map(|ro| {
                        if ro == 0.0 {
                            view! {
                                <div style=format!(
                                    "position: absolute; top: 0; left: {:.2}%; width: 2px; height: 100%; \
                                     background: rgba(234,179,8,.9); z-index: 1; pointer-events: none;",
                                    left_pct
                                )></div>
                            }.into_any()
                        } else {
                            let w = (ro / total_dur * 100.0).max(0.2);
                            view! {
                                <div style=format!(
                                    "position: absolute; top: 0; left: {:.2}%; width: {:.2}%; height: 100%; \
                                     border-radius: 4px; background: rgba(234,179,8,.4); \
                                     border: 1px solid rgba(234,179,8,.75); z-index: 1; pointer-events: none;",
                                    left_pct, w
                                )></div>
                            }.into_any()
                        }
                    })
                }).collect::<Vec<_>>()}
            </div>
            // Axis ticks
            <div style="position: relative; height: 18px; margin-top: 4px;">
                {ticks.into_iter().map(|(pct, label)| view! {
                    <span style=format!(
                        "position: absolute; left: {:.2}%; transform: translateX(-50%); \
                         font-size: .68rem; color: var(--color-sky-700); white-space: nowrap;",
                        pct
                    )>{label}</span>
                }).collect::<Vec<_>>()}
            </div>
            // Legend
            <div style="display: flex; gap: calc(var(--spacing) * 3.5); margin-top: calc(var(--spacing) * 2.5); font-size: .73rem; color: var(--color-sky-700); flex-wrap: wrap;">
                <span><span style="display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 4px; vertical-align: middle; background: rgba(168,85,247,.6);"></span>"Interstitial (X-PLAYOUT-LIMIT) · click to open playlist"</span>
                <span><span style="display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 4px; vertical-align: middle; background: rgba(234,179,8,.6);"></span>"Content consumed (X-RESUME-OFFSET)"</span>
                <span><span style="display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 4px; vertical-align: middle; background: rgba(239,68,68,.6);"></span>"Has validation errors"</span>
            </div>
        </div>
    }.into_any()
}

// ── Interstitials Cards ─────────────────────────────────────────────────────

fn dedup_interstitials(interstitials: Vec<Interstitial>) -> Vec<Interstitial> {
    // Use a Vec of (id, entry) to preserve insertion order across duplicates
    let mut ordered: Vec<String> = Vec::new();
    let mut by_id: std::collections::HashMap<String, Interstitial> = std::collections::HashMap::new();
    for it in interstitials {
        if let Some(existing) = by_id.get_mut(&it.id) {
            // Merge errors
            for e in &it.errors {
                if !existing.errors.contains(e) {
                    existing.errors.push(e.clone());
                }
            }
            // Merge optional fields — keep the first non-None value
            if existing.playout_limit.is_none() { existing.playout_limit = it.playout_limit; }
            if existing.resume_offset.is_none() { existing.resume_offset = it.resume_offset; }
            if existing.planned_duration_s.is_none() { existing.planned_duration_s = it.planned_duration_s; }
            // Merge definitions — accumulate all known variables across renditions
            for (k, v) in it.definitions {
                existing.definitions.entry(k).or_insert(v);
            }
        } else {
            ordered.push(it.id.clone());
            by_id.insert(it.id.clone(), it);
        }
    }
    // Return in stable chronological order (start_date ISO 8601 sorts lexicographically)
    ordered.sort_by(|a, b| {
        let sa = by_id.get(a).map(|it| it.start_date.as_str()).unwrap_or("");
        let sb = by_id.get(b).map(|it| it.start_date.as_str()).unwrap_or("");
        sa.cmp(sb)
    });
    ordered.into_iter().filter_map(|id| by_id.remove(&id)).collect()
}

#[component]
fn InterstitialsSection(interstitials: Vec<Interstitial>) -> impl IntoView {
    let unique = dedup_interstitials(interstitials);

    view! {
        <div style="display: flex; flex-direction: column; gap: calc(var(--spacing) * 2.5); margin-bottom: calc(var(--spacing) * 7);">
            {unique.into_iter().map(|it| {
                let has_errors = !it.errors.is_empty();
                let _status_cls = if has_errors { "fail" } else { "pass" };
                let status_color = if has_errors { "#ef4444" } else { "#22c55e" };
                let status_bg = if has_errors { "rgba(239,68,68,.15)" } else { "rgba(34,197,94,.15)" };
                let status_border = if has_errors { "rgba(239,68,68,.3)" } else { "rgba(34,197,94,.3)" };
                let status_label = if has_errors { format!("✗ {} issue{}", it.errors.len(), if it.errors.len() != 1 { "s" } else { "" }) } else { "✓ Valid".to_string() };
                let asset_url = it.asset_uri.clone().or(it.asset_list.clone());
                let asset_type = if it.asset_uri.is_some() { "X-ASSET-URI" } else if it.asset_list.is_some() { "X-ASSET-LIST" } else { "" };
                // X-ASSET-URI → open as primary playlist;
                // X-ASSET-LIST → open rendition as primary playlist with asset list in supplemental panel
                let viewer_url = if let Some(ref uri) = it.asset_uri {
                    Some(manifest_viewer_href(uri, &it.definitions))
                } else if let Some(ref list) = it.asset_list {
                    Some(manifest_viewer_href_with_asset_list(
                        &it.rendition_url,
                        list,
                        &it.id,
                        &it.definitions,
                    ))
                } else {
                    None
                };

                view! {
                    <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 4.5);">
                        <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 2.5); flex-wrap: wrap; margin-bottom: calc(var(--spacing) * 2.5);">
                            <span style="background: rgba(56,189,248,.15); color: #38bdf8; border: 1px solid rgba(56,189,248,.3); border-radius: 6px; padding: 3px 9px; font-size: .72rem; font-weight: 700; white-space: nowrap;">
                                "INTERSTITIAL"
                            </span>
                            <span style="font-weight: 700; font-size: .9rem; font-family: monospace; color: var(--color-sky-950);">
                                {if it.id.is_empty() { "(no id)".to_string() } else { it.id.clone() }}
                            </span>
                            <span style=format!(
                                "display: inline-flex; align-items: center; gap: 5px; border-radius: 20px; \
                                 padding: 4px 12px; font-size: .78rem; font-weight: 700; white-space: nowrap; \
                                 background: {}; color: {}; border: 1px solid {};",
                                status_bg, status_color, status_border
                            )>{status_label}</span>
                            {viewer_url.map(|vu| view! {
                                <a href=vu target="_blank" rel="noopener noreferrer"
                                    style="display: inline-flex; align-items: center; gap: calc(var(--spacing) * 1.25); font-size: .78rem; font-weight: 600; color: var(--color-sky-500); border: 1px solid rgba(56,189,248,.3); border-radius: 5px; padding: var(--spacing) calc(var(--spacing) * 2.5); background: rgba(56,189,248,.06); text-decoration: none;">
                                    "🔗 Open asset in HLS Viewer"
                                </a>
                            })}
                        </div>
                        <div style="display: flex; gap: calc(var(--spacing) * 4); flex-wrap: wrap; font-size: .8rem; color: var(--color-sky-700); margin-bottom: calc(var(--spacing) * 2);">
                            <span><b style="color: var(--color-sky-950);">"Rendition"</b>{format!(" {}", it.rendition)}</span>
                            <span><b style="color: var(--color-sky-950);">"START-DATE"</b>{format!(" {}", it.start_date)}</span>
                            {it.snap.map(|s| view! { <span><b style="color: var(--color-sky-950);">"X-SNAP"</b>{format!(" {}", s)}</span> })}
                            {it.resume_offset.map(|r| view! { <span><b style="color: var(--color-sky-950);">"X-RESUME-OFFSET"</b>{format!(" {}s", r)}</span> })}
                            {it.playout_limit.map(|p| view! { <span><b style="color: var(--color-sky-950);">"X-PLAYOUT-LIMIT"</b>{format!(" {}s", p)}</span> })}
                            {it.timeline_style.map(|t| view! { <span><b style="color: var(--color-sky-950);">"X-TIMELINE-STYLE"</b>{format!(" {}", t)}</span> })}
                            {it.cue.map(|c| view! { <span><b style="color: var(--color-sky-950);">"X-CUE"</b>{format!(" {}", c)}</span> })}
                        </div>
                        {asset_url.map(|url| view! {
                            <div style="font-size: .75rem; color: var(--color-sky-700); word-break: break-all; margin-top: var(--spacing);">
                                <b>{format!("{}:", asset_type)}</b>{format!(" {}", url)}
                            </div>
                        })}
                        {has_errors.then(|| view! {
                            <div style="margin-top: 8px;">
                                {it.errors.iter().map(|e| view! {
                                    <div style="font-size: .8rem; color: var(--color-red-400); padding: calc(var(--spacing) * 0.75) 0; display: flex; align-items: baseline; gap: calc(var(--spacing) * 1.5);">
                                        <span style="font-weight: 700; flex-shrink: 0;">"✗"</span>
                                        {e.clone()}
                                    </div>
                                }).collect::<Vec<_>>()}
                            </div>
                        })}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

// ── SCTE-35 Ad Breaks Section ───────────────────────────────────────────────

#[component]
fn Scte35Section(
    ad_breaks: Vec<AdBreak>,
    playlist_window_s: f64,
    definitions: std::collections::HashMap<String, String>,
) -> impl IntoView {
    // Commercial breaks only (exclude program-boundary markers from totals)
    let commercial_count = ad_breaks.iter().filter(|b| b.break_type != "program").count();
    let closed_count    = ad_breaks.iter().filter(|b| b.break_type != "program" && b.actual_duration_s.is_some()).count();
    let open_count      = ad_breaks.iter().filter(|b| b.break_type != "program" && b.actual_duration_s.is_none()).count();
    let total_actual: f64 = ad_breaks.iter()
        .filter(|b| b.break_type != "program")
        .filter_map(|b| b.actual_duration_s)
        .sum();
    let total_planned: f64 = ad_breaks.iter()
        .filter(|b| b.break_type != "program")
        .filter_map(|b| b.planned_duration_s)
        .sum();

    // Overall window: prefer the passed-in PDT-based window; fall back to the widest break span
    let break_span = {
        let first = ad_breaks.iter().filter_map(|b| b.start_offset_s).fold(f64::INFINITY, f64::min);
        let last  = ad_breaks.iter().filter_map(|b| {
            let o = b.start_offset_s?;
            let d = b.actual_duration_s.or(b.planned_duration_s).unwrap_or(0.0);
            Some(o + d)
        }).fold(0.0_f64, f64::max);
        if first.is_finite() && last > first { last } else { 0.0 }
    };
    let window_s = if playlist_window_s > 0.0 {
        playlist_window_s
    } else if break_span > 0.0 {
        break_span
    } else {
        3600.0
    };

    // Shared axis ticks (covers the full playlist window)
    let intervals = [15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0];
    let tick_interval = intervals.iter().find(|&&i| window_s / i <= 8.0).copied().unwrap_or(3600.0);
    let mut ticks: Vec<(f64, String)> = Vec::new();
    let mut t = 0.0_f64;
    while t <= window_s {
        ticks.push((t / window_s * 100.0, fmt_time(t)));
        t += tick_interval;
    }

    let breaks_for_rows = ad_breaks.clone();

    view! {
        <div style="margin-bottom: calc(var(--spacing) * 7);">

            // ── Summary bar ────────────────────────────────────────────────
            <div style="display: flex; flex-wrap: wrap; gap: calc(var(--spacing) * 2.5); margin-bottom: calc(var(--spacing) * 3.5);">
                <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 8px; padding: calc(var(--spacing) * 2.5) calc(var(--spacing) * 4); font-size: .82rem; color: var(--color-sky-700);">
                    <b style="color: var(--color-sky-950);">{ad_breaks.len()}</b>
                    {if ad_breaks.len() == 1 { " marker" } else { " markers" }}
                    " · "
                    <b style="color: var(--color-sky-950);">{commercial_count}</b>" commercial"
                    {(open_count > 0).then(|| view! {
                        <span>" · "<b style="color: #f59e0b;">{open_count}</b>" open"</span>
                    })}
                    " · "
                    <b style="color: var(--color-sky-950);">{closed_count}</b>" closed"
                </div>
                {(total_actual > 0.0).then(|| view! {
                    <div style="background: rgba(234,179,8,.12); border: 1px solid rgba(234,179,8,.35); border-radius: 8px; padding: calc(var(--spacing) * 2.5) calc(var(--spacing) * 4); font-size: .82rem; color: var(--color-sky-700);">
                        "Ad time consumed: "<b style="color: #d97706;">{fmt_time(total_actual)}</b>
                    </div>
                })}
                {(total_planned > 0.0 && (total_planned - total_actual).abs() > 1.0).then(|| view! {
                    <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 8px; padding: calc(var(--spacing) * 2.5) calc(var(--spacing) * 4); font-size: .82rem; color: var(--color-sky-700);">
                        "Planned: "<b style="color: var(--color-sky-950);">{fmt_time(total_planned)}</b>
                    </div>
                })}
            </div>

            // ── Per-marker timeline ─────────────────────────────────────────
            <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4.5) calc(var(--spacing) * 5); margin-bottom: calc(var(--spacing) * 4);">

                // Section label
                <div style="font-size: .78rem; font-weight: 700; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .06em; margin-bottom: calc(var(--spacing) * 3.5);">
                    {format!("Markers against {} playlist window", fmt_time(window_s))}
                </div>

                // Shared time axis (top, drawn once for all rows)
                <div style="position: relative; height: 16px; margin-bottom: calc(var(--spacing) * 1.5); border-bottom: 1px solid var(--color-sky-200); padding-bottom: var(--spacing);">
                    {ticks.iter().map(|(pct, label)| {
                        let pct = *pct;
                        let label = label.clone();
                        view! {
                            <span style=format!(
                                "position: absolute; left: {:.2}%; transform: translateX(-50%); \
                                 font-size: .65rem; color: var(--color-sky-300); white-space: nowrap; user-select: none;",
                                pct
                            )>{label}</span>
                        }
                    }).collect::<Vec<_>>()}
                </div>

                // One row per SCTE-35 marker
                <div style="display: flex; flex-direction: column; gap: calc(var(--spacing) * 2.5); margin-top: calc(var(--spacing) * 2);">
                    {breaks_for_rows.into_iter().map(|b| {
                        let is_program = b.break_type == "program";
                        let is_open    = b.actual_duration_s.is_none();

                        // Badge colours per type
                        let (badge_bg, badge_fg, badge_border, badge_label) = match b.break_type.as_str() {
                            "ad_break" => ("rgba(234,179,8,.15)", "#d97706", "rgba(234,179,8,.4)",   "AD BREAK"),
                            "frame_ad" => ("rgba(168,85,247,.15)", "#c084fc", "rgba(168,85,247,.4)", "FRAME AD"),
                            "program"  => ("rgba(56,189,248,.15)", "#38bdf8", "rgba(56,189,248,.4)", "PROGRAM"),
                            "chapter"  => ("rgba(34,197,94,.15)", "#22c55e", "rgba(34,197,94,.4)",   "CHAPTER"),
                            _          => ("rgba(148,163,184,.15)", "#64748b", "rgba(148,163,184,.4)", "SCTE-35"),
                        };

                        // Track fill colours (slightly more opaque than the badge)
                        let (track_bg, track_border) = match b.break_type.as_str() {
                            "ad_break" => ("rgba(234,179,8,.55)",  "rgba(234,179,8,.9)"),
                            "frame_ad" => ("rgba(168,85,247,.55)", "rgba(168,85,247,.9)"),
                            "program"  => ("rgba(56,189,248,.45)", "rgba(56,189,248,.8)"),
                            _          => ("rgba(148,163,184,.5)", "rgba(148,163,184,.9)"),
                        };

                        // Status badge
                        let (st_color, st_bg, st_border, st_label) = if is_open && !is_program {
                            ("#f59e0b", "rgba(245,158,11,.12)", "rgba(245,158,11,.35)", "⏳ open")
                        } else {
                            ("#22c55e", "rgba(34,197,94,.12)", "rgba(34,197,94,.3)", "✓ closed")
                        };

                        // Duration text
                        let dur_s = b.actual_duration_s.or(b.planned_duration_s).unwrap_or(0.0);
                        let dur_text = if let Some(a) = b.actual_duration_s {
                            fmt_time(a)
                        } else if let Some(p) = b.planned_duration_s {
                            format!("{} planned", fmt_time(p))
                        } else {
                            "—".to_string()
                        };

                        // Timeline position (relative to playlist window starting at t=0)
                        let left_pct = b.start_offset_s
                            .map(|o| (o / window_s * 100.0).clamp(0.0, 99.8))
                            .unwrap_or(0.0);
                        // Width: clamp so it never overflows the right edge
                        let w_pct = if window_s > 0.0 {
                            ((dur_s / window_s) * 100.0).clamp(0.2, 100.0 - left_pct)
                        } else { 1.0 };
                        let opacity = if is_open { "0.6" } else { "1.0" };

                        let viewer_href = manifest_viewer_href(&b.rendition_url, &definitions);
                        let tooltip = format!("{} — starts at {} — {} {}",
                            b.id,
                            b.start_date,
                            dur_text,
                            if is_open { "(open)" } else { "(closed)" }
                        );

                        // Truncate ID for display (keep last ~24 chars if long)
                        let id_short = if b.id.len() > 28 {
                            format!("…{}", &b.id[b.id.len()-24..])
                        } else { b.id.clone() };

                        view! {
                            <div style="display: flex; flex-direction: column; gap: 4px;">

                                // Header row: badge · ID · start-date · duration · status · link
                                <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap;">
                                    <span style=format!(
                                        "background: {}; color: {}; border: 1px solid {}; \
                                         border-radius: 5px; padding: 2px 8px; font-size: .68rem; \
                                         font-weight: 700; white-space: nowrap; flex-shrink: 0;",
                                        badge_bg, badge_fg, badge_border
                                    )>{badge_label}</span>
                                    <span style="font-family: monospace; font-size: .8rem; font-weight: 600; color: var(--color-sky-950); white-space: nowrap;">
                                        {id_short}
                                    </span>
                                    <span style="font-size: .75rem; color: var(--color-sky-300); white-space: nowrap;">
                                        {b.start_date.clone()}
                                    </span>
                                    <span style="font-size: .78rem; color: var(--color-sky-700); white-space: nowrap;">
                                        {dur_text}
                                    </span>
                                    <span style=format!(
                                        "font-size: .72rem; font-weight: 700; color: {}; background: {}; \
                                         border: 1px solid {}; border-radius: 20px; padding: 1px 8px; white-space: nowrap;",
                                        st_color, st_bg, st_border
                                    )>{st_label}</span>
                                    {(!b.rendition_url.is_empty()).then(|| view! {
                                        <a href=viewer_href.clone() target="_blank" rel="noopener noreferrer"
                                           style="display: inline-flex; align-items: center; gap: var(--spacing); font-size: .72rem; font-weight: 600; color: var(--color-sky-500); border: 1px solid rgba(56,189,248,.3); border-radius: 5px; padding: calc(var(--spacing) * 0.5) calc(var(--spacing) * 2); background: rgba(56,189,248,.06); text-decoration: none; white-space: nowrap; margin-left: auto;">
                                            "🔗 Open in HLS Viewer"
                                        </a>
                                    })}
                                </div>

                                // Track row: full-width bar with the break segment highlighted
                                <a href=viewer_href target="_blank" rel="noopener noreferrer"
                                   style="display: block; text-decoration: none; position: relative; height: 22px; border-radius: 5px; background: rgba(148,163,184,.08); border: 1px solid var(--color-sky-200); overflow: hidden; cursor: pointer;"
                                   title=tooltip>
                                    // Highlighted break segment
                                    <div style=format!(
                                        "position: absolute; top: 2px; bottom: 2px; left: {:.2}%; width: {:.2}%; \
                                         border-radius: 3px; background: {}; border: 1px solid {}; opacity: {};",
                                        left_pct, w_pct, track_bg, track_border, opacity
                                    )></div>
                                    // Hairline at the break start (tick mark)
                                    <div style=format!(
                                        "position: absolute; top: 0; bottom: 0; left: {:.2}%; \
                                         width: 2px; background: {}; opacity: 0.9;",
                                        left_pct, track_border
                                    )></div>
                                </a>

                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>

                // Legend
                <div style="display: flex; gap: calc(var(--spacing) * 3.5); margin-top: calc(var(--spacing) * 3.5); font-size: .72rem; color: var(--color-sky-700); flex-wrap: wrap;">
                    <span>
                        <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 4px; vertical-align: middle; background: rgba(234,179,8,.7);"></span>
                        "Commercial break"
                    </span>
                    <span>
                        <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 4px; vertical-align: middle; background: rgba(168,85,247,.7);"></span>
                        "Frame / overlay ad"
                    </span>
                    <span>
                        <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 4px; vertical-align: middle; background: rgba(56,189,248,.6);"></span>
                        "Program boundary"
                    </span>
                    <span style="opacity: .75;">"faded = open/in-progress · click track to open media playlist"</span>
                </div>
            </div>
        </div>
    }
}

// ── Delta Updates Section ───────────────────────────────────────────────────

#[component]
fn DeltaSection(
    deltas: Vec<DeltaReport>,
    definitions: std::collections::HashMap<String, String>,
) -> impl IntoView {
    view! {
        <div style="display: flex; flex-direction: column; gap: calc(var(--spacing) * 2.5); margin-bottom: calc(var(--spacing) * 7);">
            {deltas.into_iter().map(|d| {
                let has_error = d.delta_error.is_some();
                let is_audio = d.media_type == "AUDIO";
                let total = d.skipped_segments + d.delta_segment_count;
                let skip_ratio = if total > 0 { d.skipped_segments as f64 / total as f64 * 100.0 } else { 0.0 };
                let (status_color, status_bg, status_border, status_label) = if has_error {
                    ("#ef4444", "rgba(239,68,68,.15)", "rgba(239,68,68,.3)",
                     format!("✗ {}", d.delta_error.clone().unwrap_or_default()))
                } else if d.skipped_segments > 0 {
                    ("#22c55e", "rgba(34,197,94,.15)", "rgba(34,197,94,.3)",
                     format!("✓ {} segments skipped ({:.0}%)", d.skipped_segments, skip_ratio))
                } else {
                    ("#f59e0b", "rgba(245,158,11,.15)", "rgba(245,158,11,.3)",
                     "⚠ No segments skipped".to_string())
                };
                let type_badge_style = if is_audio {
                    "background: rgba(168,85,247,.15); color: #c084fc; border: 1px solid rgba(168,85,247,.3); border-radius: 6px; padding: 3px 9px; font-size: .72rem; font-weight: 700;"
                } else {
                    "background: rgba(56,189,248,.15); color: #38bdf8; border: 1px solid rgba(56,189,248,.3); border-radius: 6px; padding: 3px 9px; font-size: .72rem; font-weight: 700;"
                };
                let viewer_url = manifest_viewer_href(&d.delta_url, &definitions);

                view! {
                    <div style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 10px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 4.5);">
                        <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 2.5); flex-wrap: wrap; margin-bottom: calc(var(--spacing) * 2.5);">
                            <span style=type_badge_style>{if is_audio { "AUDIO" } else { "VIDEO" }}</span>
                            <span style="font-weight: 600; font-size: .9rem; color: var(--color-sky-950); flex: 1; min-width: 120px;">{d.name.clone()}</span>
                            <span style=format!(
                                "display: inline-flex; align-items: center; gap: 5px; border-radius: 20px; \
                                 padding: 4px 12px; font-size: .78rem; font-weight: 700; white-space: nowrap; \
                                 background: {}; color: {}; border: 1px solid {};",
                                status_bg, status_color, status_border
                            )>{status_label}</span>
                            {(!has_error).then(|| view! {
                                <a href=viewer_url.clone() target="_blank" rel="noopener noreferrer"
                                    style="display: inline-flex; align-items: center; gap: calc(var(--spacing) * 1.25); font-size: .78rem; font-weight: 600; color: var(--color-sky-500); border: 1px solid rgba(56,189,248,.3); border-radius: 5px; padding: var(--spacing) calc(var(--spacing) * 2.5); background: rgba(56,189,248,.06); text-decoration: none;">
                                    "🔗 Open delta in HLS Viewer"
                                </a>
                            })}
                        </div>
                        <div style="display: flex; gap: calc(var(--spacing) * 5); flex-wrap: wrap; font-size: .8rem; color: var(--color-sky-700); margin-bottom: calc(var(--spacing) * 2.5);">
                            <span><b style="color: var(--color-sky-950);">"CAN-SKIP-UNTIL"</b>{format!(" {:.1}s", d.can_skip_until)}</span>
                            <span><b style="color: var(--color-sky-950);">"HOLD-BACK"</b>{format!(" {:.1}s", d.hold_back)}</span>
                            <span><b style="color: var(--color-sky-950);">"CAN-BLOCK-RELOAD"</b>{if d.can_block_reload { " YES" } else { " NO" }}</span>
                            <span><b style="color: var(--color-sky-950);">"Full playlist"</b>{format!(" {} segs", d.full_segment_count)}</span>
                            {(!has_error).then(|| view! {
                                <span><b style="color: var(--color-sky-950);">"Delta playlist"</b>{format!(" {} live segs + {} skipped", d.delta_segment_count, d.skipped_segments)}</span>
                            })}
                        </div>
                        // Skip ratio bar
                        {(!has_error && total > 0).then(|| view! {
                            <div style="height: 6px; background: var(--color-sky-200); border-radius: 3px; overflow: hidden; margin-bottom: calc(var(--spacing) * 2.5);">
                                <div style=format!(
                                    "height: 100%; border-radius: 3px; background: linear-gradient(90deg, #22c55e, #16a34a); \
                                     width: {:.0}%;",
                                    skip_ratio
                                )></div>
                            </div>
                        })}
                        <div style="font-size: .75rem; color: var(--color-sky-700); word-break: break-all;">{d.delta_url.clone()}</div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

// ── Check Results Table ──────────────────────────────────────────────────────

#[component]
fn CheckResultsTable(
    groups: Vec<CheckGroup>,
    has_interstitials_data: bool,
    renditions: Vec<Rendition>,
    definitions: std::collections::HashMap<String, String>,
) -> impl IntoView {
    // Build rendition name → URL map for viewer links in drift issues
    let rend_url_map: std::collections::HashMap<String, String> = renditions.iter()
        .filter(|r| !r.url.is_empty())
        .map(|r| (r.name.clone(), r.url.clone()))
        .collect();

    let th_style = "font-size: .75rem; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .08em; padding: 0 calc(var(--spacing) * 3) calc(var(--spacing) * 2); text-align: left; font-weight: 600;";

    view! {
        <table style="width: 100%; border-collapse: separate; border-spacing: 0 6px; margin-bottom: 8px;">
            <thead>
                <tr>
                    <th style=th_style>"Check"</th>
                    <th style=th_style>"Category"</th>
                    <th style=th_style>"Reference"</th>
                    <th style=th_style>"Status"</th>
                    <th style=th_style>"Details"</th>
                </tr>
            </thead>
            <tbody>
                {groups.into_iter().map(|g| {
                    let has_issues = !g.issues.is_empty();
                    let issue_count = g.issues.len();

                    // N/A for HLS Interstitials when no interstitial data
                    let is_interstitials_check = g.name == "HLS Interstitials";
                    let show_na = is_interstitials_check && !has_interstitials_data;

                    let (pill_color, pill_bg, pill_border, pill_label) = if show_na {
                        ("#64748b", "rgba(148,163,184,.08)", "#cbd5e1", "— N/A")
                    } else {
                        match g.status.as_str() {
                            "FAIL" => ("#ef4444", "rgba(239,68,68,.15)", "rgba(239,68,68,.3)", "✗ FAIL"),
                            "WARN" => ("#f59e0b", "rgba(245,158,11,.15)", "rgba(245,158,11,.3)", "⚠ WARN"),
                            _ => ("#22c55e", "rgba(34,197,94,.15)", "rgba(34,197,94,.3)", "✓ PASS"),
                        }
                    };
                    let (expanded, set_expanded) = signal(false);
                    let rend_url_map = rend_url_map.clone();
                    let definitions = definitions.clone();

                    view! {
                        <tr
                            style=format!(
                                "background: var(--color-sky-50); transition: background .15s; {}",
                                if has_issues { "cursor: pointer;" } else { "" }
                            )
                            on:click=move |_| { if has_issues { set_expanded.set(!expanded.get()); } }
                        >
                            <td style="padding: 12px 14px; vertical-align: middle; border-radius: 8px 0 0 8px;">
                                <div style="font-weight: 600; font-size: .92rem;">{g.name.clone()}</div>
                            </td>
                            <td style="padding: 12px 14px; vertical-align: middle;">
                                <div style="font-size: .75rem; color: var(--color-sky-700);">{g.section.clone()}</div>
                            </td>
                            <td style="padding: 12px 14px; vertical-align: middle;">
                                <span style="font-size: .72rem; color: var(--color-sky-500); background: rgba(56,189,248,.1); border-radius: 4px; padding: calc(var(--spacing) * 0.5) calc(var(--spacing) * 1.75); display: inline-block; white-space: nowrap;">
                                    {g.reference.clone()}
                                </span>
                            </td>
                            <td style="padding: 12px 14px; vertical-align: middle;">
                                <span style=format!(
                                    "display: inline-flex; align-items: center; gap: 5px; border-radius: 20px; \
                                     padding: 4px 12px; font-size: .78rem; font-weight: 700; white-space: nowrap; \
                                     background: {}; color: {}; border: 1px solid {};",
                                    pill_bg, pill_color, pill_border
                                )>{pill_label}</span>
                            </td>
                            <td style="padding: 12px 14px; vertical-align: middle; border-radius: 0 8px 8px 0;">
                                {has_issues.then(|| view! {
                                    <button style="background: none; border: none; color: var(--color-sky-700); cursor: pointer; font-size: .8rem; padding: 0; display: flex; align-items: center; gap: var(--spacing);">
                                        {move || if expanded.get() {
                                            format!("▲ {} issue{}", issue_count, if issue_count != 1 { "s" } else { "" })
                                        } else {
                                            format!("▼ {} issue{}", issue_count, if issue_count != 1 { "s" } else { "" })
                                        }}
                                    </button>
                                })}
                                {(!has_issues).then(|| view! {
                                    <span style="color: var(--color-sky-700); font-size: .8rem;">"—"</span>
                                })}
                            </td>
                        </tr>
                        {move || expanded.get().then(|| {
                            let issues = g.issues.clone();
                            let rend_url_map = rend_url_map.clone();
                            let definitions = definitions.clone();
                            view! {
                                <tr>
                                    <td colspan="5" style="padding: 0 14px 10px;">
                                        <div style="background: var(--color-sky-100); border-radius: 8px; padding: calc(var(--spacing) * 3) calc(var(--spacing) * 3.5); border: 1px solid var(--color-sky-200);">
                                            {issues.into_iter().map(|iss| {
                                                let (sev_color, sev_icon) = match iss.severity {
                                                    Severity::Error => ("#ef4444", "✗"),
                                                    Severity::Warn => ("#f59e0b", "⚠"),
                                                    Severity::Info => ("#60a5fa", "ℹ"),
                                                };
                                                let seg_label = if iss.count > 1 {
                                                    format!("Segments {}–{} (×{})", iss.seg_first, iss.seg_last, iss.count)
                                                } else if iss.segment_index >= 0 {
                                                    format!("Segment {}", iss.segment_index)
                                                } else {
                                                    "Global".to_string()
                                                };
                                                // Check if this is a drift issue that should show viewer links
                                                let is_drift = is_drift_issue_message(&iss.message);
                                                let viewer_links: Vec<(String, String)> = if is_drift {
                                                    let mut links = Vec::new();
                                                    if let Some(ref ra) = iss.rendition_a
                                                        && let Some(url) = rend_url_map.get(ra) {
                                                            links.push((ra.clone(), manifest_viewer_href(url, &definitions)));
                                                        }
                                                    if let Some(ref rb) = iss.rendition_b
                                                        && let Some(url) = rend_url_map.get(rb) {
                                                            links.push((rb.clone(), manifest_viewer_href(url, &definitions)));
                                                        }
                                                    links
                                                } else {
                                                    Vec::new()
                                                };

                                                view! {
                                                    <div style=format!(
                                                        "border-left: 3px solid {}; padding: 10px 12px; \
                                                         margin-bottom: 8px; border-radius: 0 6px 6px 0; \
                                                         background: rgba(255,255,255,.04);",
                                                        sev_color
                                                    )>
                                                        <div style=format!(
                                                            "font-size: .7rem; font-weight: 800; text-transform: uppercase; \
                                                             letter-spacing: .06em; margin-bottom: 4px; color: {};",
                                                            sev_color
                                                        )>
                                                            {format!("{} {}", sev_icon, iss.severity)}
                                                        </div>
                                                        <div style="font-size: .85rem; line-height: 1.6; color: var(--color-sky-950);">
                                                            {iss.message.clone()}
                                                        </div>
                                                        {iss.uri_note.as_ref().map(|n| view! {
                                                            <div style="font-size: .78rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 1.5); padding-top: calc(var(--spacing) * 1.5); border-top: 1px solid var(--color-sky-200); line-height: 1.5;">
                                                                {format!("📎 {}", n)}
                                                            </div>
                                                        })}
                                                        <div style="font-size: .75rem; color: var(--color-sky-700); margin-top: var(--spacing);">
                                                            {seg_label}
                                                            {iss.rendition_a.as_ref().map(|ra| {
                                                                if let Some(rb) = &iss.rendition_b {
                                                                    view! { <span style="color: var(--color-sky-500);">{format!(" · {} ↔ {}", ra, rb)}</span> }.into_any()
                                                                } else {
                                                                    view! { <span style="color: var(--color-sky-500);">{format!(" · {}", ra)}</span> }.into_any()
                                                                }
                                                            })}
                                                        </div>
                                                        {(!viewer_links.is_empty()).then(|| view! {
                                                            <div style="display: flex; gap: 8px; flex-wrap: wrap; margin-top: 6px;">
                                                                {viewer_links.into_iter().map(|(name, url)| view! {
                                                                    <a href=url target="_blank" rel="noopener noreferrer"
                                                                        style="display: inline-flex; align-items: center; gap: calc(var(--spacing) * 1.25); font-size: .78rem; font-weight: 600; color: var(--color-sky-500); border: 1px solid rgba(56,189,248,.3); border-radius: 5px; padding: var(--spacing) calc(var(--spacing) * 2.5); background: rgba(56,189,248,.06); text-decoration: none;">
                                                                        {format!("🔗 View {}", name)}
                                                                    </a>
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        })}
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </td>
                                </tr>
                            }
                        })}
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── compress_codecs ───────────────────────────────────────────────────────

    #[test]
    fn compress_codecs_empty_returns_dash() {
        assert_eq!(compress_codecs(""), "—");
    }

    #[test]
    fn compress_codecs_h264_aac() {
        assert_eq!(compress_codecs("avc1.64001f,mp4a.40.2"), "H.264 + AAC");
    }

    #[test]
    fn compress_codecs_hevc_he_aac() {
        assert_eq!(compress_codecs("hvc1.2.4.L153,mp4a.40.5"), "HEVC + HE-AAC");
    }

    #[test]
    fn compress_codecs_deduplicated() {
        // Two hvc1 tokens should collapse to one "HEVC"
        assert_eq!(compress_codecs("hvc1.1.6.L120,hvc1.2.4.L153"), "HEVC");
    }

    #[test]
    fn compress_codecs_unknown_passthrough() {
        assert_eq!(compress_codecs("unknown.codec"), "unknown.codec");
    }

    #[test]
    fn compress_codecs_av1() {
        assert_eq!(compress_codecs("av01.0.08M.10"), "AV1");
    }

    #[test]
    fn compress_codecs_dolby_vision_via_dvh1() {
        // dvh1 prefix maps to "HEVC" in compress_codecs (same as hvc1)
        assert_eq!(compress_codecs("dvh1.08.07"), "HEVC");
    }

    // ── format_bandwidth ──────────────────────────────────────────────────────

    #[test]
    fn format_bandwidth_zero_returns_dash() {
        assert_eq!(format_bandwidth(0), "—");
    }

    #[test]
    fn format_bandwidth_kbps_below_one_million() {
        assert_eq!(format_bandwidth(3_000_000), "3.00 Mbps");
    }

    #[test]
    fn format_bandwidth_kbps_for_values_below_one_million() {
        assert_eq!(format_bandwidth(500_000), "500 kbps");
    }

    #[test]
    fn format_bandwidth_rounds_mbps() {
        assert_eq!(format_bandwidth(1_500_000), "1.50 Mbps");
    }

    // ── fmt_time ──────────────────────────────────────────────────────────────

    #[test]
    fn fmt_time_seconds_only() {
        assert_eq!(fmt_time(45.0), "0:45");
    }

    #[test]
    fn fmt_time_minutes_and_seconds() {
        assert_eq!(fmt_time(125.0), "2:05");
    }

    #[test]
    fn fmt_time_hours() {
        assert_eq!(fmt_time(3661.0), "1:01:01");
    }

    #[test]
    fn fmt_time_zero() {
        assert_eq!(fmt_time(0.0), "0:00");
    }

    // ── manifest_viewer_href ──────────────────────────────────────────────────

    #[test]
    fn is_drift_issue_message_matches_check_output() {
        assert!(is_drift_issue_message(
            "EXTINF drift at MSN 42: 'hi' has 6.006s vs 'lo' has 6.000s (diff=0.006s)"
        ));
        assert!(is_drift_issue_message(
            "Cumulative EXTINF drift across renditions: 0.120s (tolerance=0.100s)"
        ));
        assert!(
            !is_drift_issue_message("EXTINF duration must be ≤ TARGETDURATION"),
            "unrelated EXTINF messages must not get drift viewer links"
        );
    }

    #[test]
    fn report_definitions_prefers_master_then_media() {
        let mut master_defs = HashMap::new();
        master_defs.insert("TOKEN".into(), "from-master".into());
        let mut media_defs = HashMap::new();
        media_defs.insert("TOKEN".into(), "from-media".into());

        let mut with_master = ValidationReport::new();
        with_master.master_url = "https://ex.com/master.m3u8".into();
        with_master.master = Some(MasterPlaylist {
            url: "https://ex.com/master.m3u8".into(),
            raw_content: String::new(),
            version: 6,
            variants: Vec::new(),
            media_renditions: Vec::new(),
            definitions: master_defs,
            independent_segments: false,
            http_meta: PlaylistHttpMeta::default(),
        });
        assert_eq!(
            report_definitions(&with_master).get("TOKEN").map(String::as_str),
            Some("from-master")
        );

        let mut media_only = ValidationReport::new();
        media_only.master_url = "https://ex.com/media.m3u8".into();
        let mut pl = MediaPlaylist::new("media".into(), "https://ex.com/media.m3u8".into());
        pl.definitions = media_defs;
        media_only.playlists.push(pl);
        assert_eq!(
            report_definitions(&media_only).get("TOKEN").map(String::as_str),
            Some("from-media")
        );
    }

    #[test]
    fn manifest_viewer_href_no_definitions() {
        let url = "https://cdn.example.com/master.m3u8";
        let defs = HashMap::new();
        let href = manifest_viewer_href(url, &defs);
        assert!(href.starts_with("/hls-manifest-viewer/?playlist_url="));
        assert!(href.contains("cdn%2Eexample%2Ecom"));
    }

    #[test]
    fn manifest_viewer_href_with_definitions_includes_imported_definitions() {
        let url = "https://cdn.example.com/master.m3u8";
        let mut defs = HashMap::new();
        defs.insert("TOKEN".to_string(), "abc".to_string());
        let href = manifest_viewer_href(url, &defs);
        assert!(href.contains("imported_definitions="), "expected imported_definitions in href");
    }

    #[test]
    fn manifest_viewer_href_substitutes_hls_variables() {
        let url = "https://cdn.example.com/{$TOKEN}/master.m3u8";
        let mut defs = HashMap::new();
        defs.insert("TOKEN".to_string(), "live".to_string());
        let href = manifest_viewer_href(url, &defs);
        // After substitution "live" replaces "{$TOKEN}"
        assert!(!href.contains("%7B%24TOKEN%7D"), "variable must be substituted, not left as-is");
        assert!(href.contains("live"), "substituted value must appear in href");
    }

    #[test]
    fn manifest_viewer_href_with_asset_list_uses_rendition_as_primary() {
        let rendition = "https://cdn.example.com/rendition.m3u8";
        let asset_list = "https://ads.example.com/ads.json";
        let defs = HashMap::new();
        let href = manifest_viewer_href_with_asset_list(rendition, asset_list, "ad-1", &defs);
        // Primary playlist_url must be the rendition, not the asset list
        assert!(href.contains("playlist_url="), "must have playlist_url");
        assert!(href.contains("cdn%2Eexample%2Ecom"), "rendition must be the primary URL");
        assert!(href.contains("supplemental_view_context="), "asset list must be in supplemental context");
        assert!(href.contains("ASSET_LIST"), "supplemental context must be ASSET_LIST type");
    }
}
