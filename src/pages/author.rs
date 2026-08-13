use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::utils::author::{run_author_report, AuthorOptions, AuthorProfile, AuthorReport};
use crate::utils::validator::types::{CheckGroup, Severity};

#[component]
pub fn Author() -> impl IntoView {
    let (url_input, set_url_input) = signal(String::new());
    let (profile, set_profile) = signal("None".to_string());
    let (deep_checks, set_deep_checks) = signal(false);
    let (report, set_report) = signal(None::<AuthorReport>);
    let (error_msg, set_error_msg) = signal(None::<String>);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let url = url_input.get();
        if url.is_empty() {
            return;
        }
        let selected = profile.get();
        let deep = deep_checks.get();
        set_loading.set(true);
        set_error_msg.set(None);
        set_report.set(None);
        leptos::task::spawn_local(async move {
            let options = AuthorOptions {
                profile: AuthorProfile::parse(&selected),
                deep_checks: deep,
            };
            match run_author_report(&url, options).await {
                Ok(r) => set_report.set(Some(r)),
                Err(e) => set_error_msg.set(Some(format!("Author check failed: {}", e))),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="body-content" style="margin-bottom: 2em;">
            <h1 class="body-content">"Check against the Apple HLS Authoring Spec"</h1>
            <p class="body-content body-text">
                "Enter a master or media playlist URL to run the Apple HLS Authoring Specification rules — codecs, bitrate ladders, segmentation, trick play, accessibility, content protection and more. Phase B always probes init segments (and samples I-frame/WebVTT lightly). Pick a platform profile to apply its amendments, and enable deep checks to sample more media segments for measured bitrate and bitstream heuristics."
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
                            {move || if loading.get() { "⏳ Checking..." } else { "▶ Check authoring" }}
                        </button>
                    </div>
                    <div style="display: flex; flex-wrap: wrap; gap: calc(var(--spacing) * 5); margin-top: calc(var(--spacing) * 4.5); align-items: center;">
                        <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 2); font-size: .875rem; color: var(--color-sky-700);">
                            <label for="author-profile">"Platform profile"</label>
                            <select
                                id="author-profile"
                                style="background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 6px; color: var(--color-sky-950); padding: calc(var(--spacing) * 1.25) calc(var(--spacing) * 2.5); font-size: .875rem; outline: none;"
                                prop:value=move || profile.get()
                                on:change=move |ev| set_profile.set(event_target_value(&ev))
                            >
                                <option value="None">"General (no amendments)"</option>
                                <option value="iOS">"iOS"</option>
                                <option value="tvOS">"tvOS"</option>
                                <option value="macOS">"macOS"</option>
                                <option value="visionOS">"visionOS"</option>
                                <option value="AirPlay2">"AirPlay 2"</option>
                            </select>
                        </div>
                        <div style="display: flex; align-items: center; gap: calc(var(--spacing) * 2); font-size: .875rem; color: var(--color-sky-700);">
                            <input
                                type="checkbox"
                                id="deep-author"
                                prop:checked=move || deep_checks.get()
                                on:change=move |ev| {
                                    if let Some(t) = ev.target()
                                        && let Ok(input) = t.dyn_into::<web_sys::HtmlInputElement>()
                                    {
                                        set_deep_checks.set(input.checked());
                                    }
                                }
                            />
                            <label for="deep-author">"Deep checks (downloads media segments)"</label>
                        </div>
                    </div>
                </form>
                {move || loading.get().then(|| view! {
                    <div style="margin-top: calc(var(--spacing) * 3);">
                        <div style="height: 3px; background: var(--color-sky-200); border-radius: 2px; overflow: hidden;">
                            <div style="width: 40%; height: 100%; background: linear-gradient(90deg, var(--color-sky-300), var(--color-sky-500)); animation: progress 1.5s ease-in-out infinite; border-radius: 2px;"></div>
                        </div>
                        <div style="font-size: .85rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 1.5);">"Fetching playlists, probing segments and applying Authoring Spec rules…"</div>
                    </div>
                })}
                {move || error_msg.get().map(|e| view! {
                    <div style="margin-top: calc(var(--spacing) * 3); padding: calc(var(--spacing) * 3.5) calc(var(--spacing) * 4.5); background: rgba(239,68,68,.15); border: 1px solid var(--color-red-400); border-radius: 8px; color: var(--color-red-400); font-size: .9rem;">
                        {format!("⚠ {}", e)}
                    </div>
                })}
            </div>
        </div>

        {move || report.get().map(|r| view! { <AuthorResults report=r /> })}
    }
}

#[component]
fn AuthorResults(report: AuthorReport) -> impl IntoView {
    let is_pass = report.result == "PASS";
    let (banner_color, banner_bg, banner_label) = if is_pass {
        ("#22c55e", "rgba(34,197,94,.12)", "✓ PASS")
    } else {
        ("#ef4444", "rgba(239,68,68,.12)", "✗ FAIL")
    };
    let summary = format!(
        "{} playlist{} · {} init probe{} · {} segment sample{} · {} WebVTT sample{} · deep checks {} · {} ms",
        report.playlist_count,
        if report.playlist_count == 1 { "" } else { "s" },
        report.init_probe_count,
        if report.init_probe_count == 1 { "" } else { "s" },
        report.segment_sample_count,
        if report.segment_sample_count == 1 { "" } else { "s" },
        report.webvtt_sample_count,
        if report.webvtt_sample_count == 1 { "" } else { "s" },
        if report.deep_checks { "on" } else { "off" },
        report.elapsed_ms,
    );
    let total_findings = report.issues.len();

    view! {
        <div class="body-content" style="margin-bottom: 3em;">
            <div style=format!(
                "display: flex; align-items: center; gap: calc(var(--spacing) * 4); flex-wrap: wrap; \
                 padding: calc(var(--spacing) * 4) calc(var(--spacing) * 5); border-radius: 12px; \
                 border: 1px solid {}; background: {}; margin-bottom: calc(var(--spacing) * 6);",
                banner_color, banner_bg
            )>
                <span style=format!("font-size: 1.1rem; font-weight: 800; color: {};", banner_color)>
                    {banner_label}
                </span>
                <span style="font-size: .85rem; color: var(--color-sky-700); word-break: break-all;">
                    {report.url.clone()}
                </span>
                <span style="font-size: .8rem; color: var(--color-sky-700); margin-left: auto; white-space: nowrap;">
                    {format!("{} finding{}", total_findings, if total_findings == 1 { "" } else { "s" })}
                </span>
            </div>

            <div style="display: flex; gap: calc(var(--spacing) * 3); flex-wrap: wrap; margin-bottom: calc(var(--spacing) * 6);">
                <StatCard value=report.total_errors.to_string() label="Errors" color="#ef4444" />
                <StatCard value=report.total_warnings.to_string() label="Warnings" color="#f59e0b" />
                <StatCard value=report.total_info.to_string() label="Info" color="#60a5fa" />
                <StatCard value=report.profile.clone() label="Profile" color="var(--color-sky-700)" />
            </div>

            <div style="color: var(--color-sky-700); font-size: .82rem; margin-bottom: calc(var(--spacing) * 5); padding: calc(var(--spacing) * 3) calc(var(--spacing) * 4); background: var(--color-sky-50); border-radius: 8px; border: 1px solid var(--color-sky-200);">
                <div style="font-weight: 700; margin-bottom: calc(var(--spacing) * 1.5);">{summary}</div>
                <ul style="margin: 0; padding-left: 1.2em;">
                    {report.probe_notes.clone().into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}
                </ul>
            </div>

            <AuthorCheckTable groups=report.check_groups.clone() />
        </div>
    }
}

#[component]
fn StatCard(value: String, label: &'static str, color: &'static str) -> impl IntoView {
    view! {
        <div style="flex: 1; min-width: 100px; padding: calc(var(--spacing) * 4) calc(var(--spacing) * 5); background: var(--color-sky-50); border: 1px solid var(--color-sky-200); border-radius: 12px; text-align: center;">
            <div style=format!("font-size: 1.5rem; font-weight: 700; color: {};", color)>{value}</div>
            <div style="font-size: .8rem; color: var(--color-sky-700); margin-top: calc(var(--spacing) * 0.5);">{label}</div>
        </div>
    }
}

#[component]
fn AuthorCheckTable(groups: Vec<CheckGroup>) -> impl IntoView {
    let th_style = "font-size: .75rem; color: var(--color-sky-700); text-transform: uppercase; letter-spacing: .08em; padding: 0 calc(var(--spacing) * 3) calc(var(--spacing) * 2); text-align: left; font-weight: 600;";

    view! {
        <table style="width: 100%; border-collapse: separate; border-spacing: 0 6px;">
            <thead>
                <tr>
                    <th style=th_style>"Check"</th>
                    <th style=th_style>"Reference"</th>
                    <th style=th_style>"Status"</th>
                    <th style=th_style>"Details"</th>
                </tr>
            </thead>
            <tbody>
                {groups.into_iter().map(|g| {
                    let has_issues = !g.issues.is_empty();
                    let issue_count = g.issues.len();
                    let (pill_color, pill_bg, pill_border, pill_label) = match g.status.as_str() {
                        "FAIL" => ("#ef4444", "rgba(239,68,68,.15)", "rgba(239,68,68,.3)", "✗ FAIL"),
                        "WARN" => ("#f59e0b", "rgba(245,158,11,.15)", "rgba(245,158,11,.3)", "⚠ WARN"),
                        _ => ("#22c55e", "rgba(34,197,94,.15)", "rgba(34,197,94,.3)", "✓ PASS"),
                    };
                    let (expanded, set_expanded) = signal(false);

                    view! {
                        <tr
                            style=format!(
                                "background: var(--color-sky-50); {}",
                                if has_issues { "cursor: pointer;" } else { "" }
                            )
                            on:click=move |_| { if has_issues { set_expanded.set(!expanded.get()); } }
                        >
                            <td style="padding: 12px 14px; vertical-align: middle; border-radius: 8px 0 0 8px;">
                                <div style="font-weight: 600; font-size: .92rem;">{g.name.clone()}</div>
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
                                            format!("▲ {} finding{}", issue_count, if issue_count != 1 { "s" } else { "" })
                                        } else {
                                            format!("▼ {} finding{}", issue_count, if issue_count != 1 { "s" } else { "" })
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
                            view! {
                                <tr>
                                    <td colspan="4" style="padding: 0 14px 10px;">
                                        <div style="background: var(--color-sky-100); border-radius: 8px; padding: calc(var(--spacing) * 3) calc(var(--spacing) * 3.5); border: 1px solid var(--color-sky-200);">
                                            {issues.into_iter().map(|iss| {
                                                let (sev_color, sev_icon) = match iss.severity {
                                                    Severity::Error => ("#ef4444", "✗"),
                                                    Severity::Warn => ("#f59e0b", "⚠"),
                                                    Severity::Info => ("#60a5fa", "ℹ"),
                                                };
                                                view! {
                                                    <div style=format!(
                                                        "border-left: 3px solid {}; padding: 10px 12px; margin-bottom: 8px; \
                                                         border-radius: 0 6px 6px 0; background: rgba(255,255,255,.04);",
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
                                                        {iss.rendition_a.as_ref().map(|ra| view! {
                                                            <div style="font-size: .75rem; color: var(--color-sky-700); margin-top: var(--spacing);">
                                                                {format!("· {}", ra)}
                                                            </div>
                                                        })}
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </td>
                                </tr>
                            }
                        })}
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}

