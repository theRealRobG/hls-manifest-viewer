# Inspect Timing — manual browser test matrix

Everything in `src/utils/timing.rs` and `src/utils/timing/live.rs` that can be tested
without a browser is covered by `cargo test --lib`. What cannot be tested there is the part
that matters most: whether a real browser, against a real CDN, gives the fields the table
claims it does. Resource Timing is the whole problem — its availability turns on
`Timing-Allow-Origin`, its buffer behaves differently per engine, and `cache: no-store` is
honoured to different depths. The live path adds a second untestable half: whether an origin
that advertises `CAN-BLOCK-RELOAD=YES` actually holds the request, which plenty do not.

Run this matrix by hand before trusting a number from the Timing table.

## Streams to test with

| Stream | Why |
| --- | --- |
| Same-origin VOD | The only case where `Timing-Allow-Origin` is not needed for the connect fields, so it is the control. |
| Cross-origin VOD **with** `Timing-Allow-Origin: *` | True TTFB and cold/warm should both appear. |
| Cross-origin VOD **without** `Timing-Allow-Origin` | True TTFB and connection state must read as unavailable, never `0 ms`. |
| VOD using `EXT-X-BYTERANGE` | Range honoured (206) and range ignored (200) both need checking. |
| Muxed (MPEG-TS, no `EXT-X-MAP`) VOD | A pair must be one request, not two. |
| Demuxed fMP4 VOD with a `DEFAULT` audio rendition | A pair must be two concurrent requests. |
| Live playlist (no `EXT-X-ENDLIST`), segments only | The edge is watched for a whole segment. `_HLS_msn` only, never `_HLS_part`. |
| LL-HLS with `EXT-X-PART-INF` and `CAN-BLOCK-RELOAD=YES` | Blocking reloads carry `_HLS_msn` **and** `_HLS_part`; the sampled item is a part, and its percentages are against `PART-TARGET`. |
| LL-HLS with parts inside one file (`EXT-X-PART` with `BYTERANGE`) | Each new part must be recognised as new even though it shares the segment's URI. |
| Live playlist advertising **no** `CAN-BLOCK-RELOAD` | The edge is polled, no query parameter is added anywhere, and the polling note appears. |
| Live playlist that has stopped publishing | The wait must time out, the report must **not** read as cancelled, and the timeout note must appear. |
| Encrypted VOD (`EXT-X-KEY`) | Bytes must be timed as opaque; no key request may appear. |
| Playlist with a `GAP` segment | The GAP must never be fetched. |
| A URL that 404s, and a cross-origin URL with no CORS headers | Rows must read "failed" with no duration. |

## Browsers

Run each check in Chrome/Edge (Blink), Safari (WebKit) and Firefox (Gecko), desktop at
minimum. Note the version tested.

## Checks

| # | Check | Expected |
| --- | --- | --- |
| 1 | Timing is unchecked on first load | None of the three Timing items is selected; probing does no extra requests. Select-all on the Timing box ticks all three, and Deselect-all clears all three. |
| 2 | Playlist requests input | Accepts 1–20; typing 0 or 99 clamps to 1 or 20 on blur, and the run makes exactly that many. |
| 3 | Media pairs input | Accepts 1–10; clamps the same way. The same count is how many VOD pairs are strided and how many live-edge samples are waited for, whichever media check ran. |
| 4 | Request count, master URL | 1 multivariant row + N media playlist rows. |
| 5 | Request count, media playlist URL | N media playlist rows, no multivariant row, and the "URL probed is a media playlist" note. With **Media sample timing** on, the segment rows read "Media segment (muxing unknown)" — never "Muxed segment" — and the muxing note appears. |
| 6 | `no-store` | DevTools network panel shows no "(disk cache)" / "(memory cache)" for any Timing request, on repeat runs too. |
| 7 | No cache busting | Every Timing request URL is byte-identical to the playlist's URI: no added query parameters. The one exception is a live blocking reload, which adds `_HLS_msn` and `_HLS_part` and nothing else — never `_HLS_skip`, never a random parameter. |
| 8 | Resource Timing total | The RT total column is populated for cross-origin requests **without** `Timing-Allow-Origin`. This is the authoritative column and must not be empty just because TAO is absent. |
| 9 | True TTFB with TAO | Populated, and plausibly smaller than the RT total. |
| 10 | True TTFB without TAO | Empty (em dash). **Never `0.0 ms`.** |
| 11 | Connection state without TAO | "Unknown (no Timing-Allow-Origin)". |
| 12 | Connection state with TAO | "Warm (connection reused)" for the later playlist repeats, since Inspect already opened the connection. A first row reading "Cold" should be treated with suspicion, not as a cold-start measurement. |
| 13 | App-observed vs RT | The app-observed total is greater than or equal to the RT total on media rows (it includes the `Uint8Array` copy). If it is consistently smaller, the entry matching is wrong. |
| 14 | Entry matching under concurrency | On a demuxed stream, the video and audio rows of one pair have different RT totals and neither is missing. Both requests overlap in DevTools. |
| 15 | Pairs are serial | In DevTools, pair 2 starts after pair 1's slower half finishes. |
| 16 | Stop mid-playlist-phase | Progress stops, remaining requests are cancelled in DevTools, completed rows keep their durations, and the report says it was stopped. |
| 17 | Stop mid-pair | The in-flight pair's rows read "cancelled" with no duration — not "failed". |
| 18 | Re-probe while Timing runs | The previous run's requests are cancelled in DevTools and its results never appear. |
| 19 | BYTERANGE honoured | Status 206, and the byte count matches the range length. |
| 20 | BYTERANGE ignored | Status shows "200 (range ignored)", both percentage cells read "— (range ignored)" rather than a ratio, and the range-ignored note appears. |
| 20a | Demuxed variant whose AUDIO group has no fetchable rendition | Pairs run video-only, one request each, and the note saying no DEFAULT audio rendition with a URI was found appears. No rendition from another group is fetched. |
| 21 | Redirected media | The URI cell shows `requested → final`, and the row still has an RT total. Entries are named by the requested URL, so a missing RT total here means the matching broke. |
| 22 | Compressed playlist | The bytes cell shows the app count and a smaller `encodedBodySize` (or `Content-Length`), with the source named. |
| 23 | Live stream, both media checks **off** | Playlist rows only, exactly as for VOD. No live note is needed, because nothing about the live edge was claimed. |
| 23a | Live stream, **Media sample timing** on, **Live edge timing** off | The VOD check is VOD-only, so no media is fetched: DevTools shows no segment or part request, and no request anywhere carries `_HLS_msn` or `_HLS_part`. The playlist rows still stand, and the report carries the note saying VOD sampling was asked for on a playlist with no `EXT-X-ENDLIST`, pointing at the Live edge check. |
| 23b | VOD stream, **Live edge timing** on, **Media sample timing** off | No wait and no media request: the playlist has `EXT-X-ENDLIST`, so there is no edge. The playlist rows stand, and the note says live timing was asked for on a finished asset and points at the VOD check. |
| 23c | Both media checks on | Only the one the playlist can answer runs — a stride on VOD, edge samples on live — never both. The other check's decline note appears, so the missing half is visible rather than silently dropped. |
| 23d | Live stream, **Live edge timing** on, `CAN-BLOCK-RELOAD=YES` | Each sample shows one or more reload rows carrying `_HLS_msn` (plus `_HLS_part` where the playlist has `EXT-X-PART-INF`) followed by the media row for the item that appeared. Those rows are classified "Live playlist reload (held)", never "Media playlist" — the hold is a wait on an author, not a download, and must never be read as the part's TTFB. Check the medians table too: the median for "Media playlist" covers the repeat phase only, with no hold inside it. |
| 23e | Live stream, **Live edge timing** on, no `CAN-BLOCK-RELOAD` | No query parameter is added to any request. Reload rows appear roughly one target duration (or `PART-TARGET`, floor one second) apart, and the polling note says an item may have been published up to one interval before this tab saw it. |
| 23f | LL-HLS parts vs segments | With `EXT-X-PART-INF`, the sampled URI is an `EXT-X-PART` URI and the target-duration cell reads `PART-TARGET`. Without it, the sampled URI is a segment and the cell reads `TARGETDURATION`. A part is never sampled from a parent segment that has already completed. |
| 23g | Observation lag | Populated on live media rows only; em dash on playlist rows and on every VOD row. Compare it by hand against the reload row's timestamp plus the media row's total: it must be at least the media total and never negative. |
| 23h | Observation lag against target | `% of target (observation lag)` divides by `PART-TARGET` for a part and `TARGETDURATION` for a segment. It is empty, not zero, when the playlist declares neither. |
| 23i | True TTFB on a part request | Still needs `Timing-Allow-Origin` on the **media** response. The blocking reload's own duration is not a substitute and must not appear in the TTFB column. |
| 23j | Live wait times out | Point at a live playlist that has stopped publishing (or an origin that ignores `_HLS_msn`). The wait's last reload row reads "failed: this request was still open after … ms …", the report is **not** marked cancelled, the timeout note appears, and no further live samples are attempted. The same row must appear on the polled path, where a request that never answers is given up on at the same ceiling rather than hanging the run. |
| 23k | Stop during a blocking hold | Press Stop while a reload is held. DevTools shows the held request cancelled immediately; the row reads "cancelled", not "failed"; the report says it was stopped. |
| 23l | Live GAP part | A trailing `GAP=YES` part is never fetched, and the next `_HLS_part` asked for is the index **after** the GAP, not the GAP's own index (which would return a playlist already in hand). Where a GAP lands on the index a wait is already in flight for, the following reload within that same wait must carry a **higher** `_HLS_part` than the one before it: repeating the directive would burn the whole ceiling on a body already in hand. |
| 23m | Preload hints | No request is ever made to an `EXT-X-PRELOAD-HINT` URI: those bytes may not exist yet, so timing them would measure the server waiting for an encoder. |
| 23n | Live demuxed audio | The two *waits* are concurrent in DevTools. The two downloads need not overlap: each half fetches as soon as its own playlist named the item, so the slower edge cannot push its leftover wait into the other's observation lag. If the audio edge produces nothing, the sample is video-only and the pair note says which rendition and why. |
| 24 | Encrypted stream | Media rows present with byte counts; no request to the `EXT-X-KEY` URI anywhere in DevTools. |
| 25 | GAP segment | The GAP segment's URI appears in no request. |
| 26 | Failed request | Row reads "failed: …", all duration cells are em dashes, and the run continues. |
| 27 | Byte cap | With a high-bitrate stream and 10 pairs, the run stops once ~150 MB has arrived, the remaining pairs appear as "skipped: byte cap reached before this pair", and the cap note names the pair it stopped after. |
| 28 | Median | With 4 playlist repeats, the median matches the middle two rows by hand. No mean or standard deviation is shown anywhere. |
| 29 | Missing `performance.now()` | Hard to force in a normal browser; if a context without `Performance` is available, Timing must decline with a message rather than render zeros. |
| 30 | Provenance panel | The caveat list renders below the tables on every run, including a run that measured nothing. |

## Known limitations to confirm rather than fix

- The first Timing sample is never a true cold start: Inspect has already fetched the
  playlists over the same connection.
- A CDN hit and a CDN miss are indistinguishable, because no cache-busting parameter is
  added.
- A range request's preflight, where one happens, is inside the measured time.
- Observation lag starts when **this tab** read a playlist body naming the URI, not when the
  origin added it. No browser can see the add. The figure is therefore an upper bound on
  what the fetch cost (it also contains this tab's own scheduling between reading the
  playlist and issuing the request) and a floor on the interval a client would experience
  from publication to bytes in hand.
- A blocking reload that returns immediately means the item was already published before
  the reload was sent, so that sample's lag measures this tab's lateness rather than the
  origin's. The reload row's own duration is what tells the two apart, which is why those
  rows are in the table.
- The first live sample runs straight after the playlist repeat phase, so its baseline
  playlist can be a few seconds old and its "appearance" may be of something published
  during that gap. Later samples diff against the body of the previous reload.
- On the polling path the appearance is seen up to one poll interval late, so the lag is
  shorter than a blocking client's would be.
- `EXT-X-PROGRAM-DATE-TIME` is never used as a latency. It is an authoring clock.
- A wait is capped at three hold-backs (`PART-HOLD-BACK` when waiting for a part), clamped
  to at most a minute. A stream slower than its own declared hold-back reads as a timeout,
  which is a statement about the wait and not about the origin.
