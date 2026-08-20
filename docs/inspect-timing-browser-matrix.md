# Inspect Timing — manual browser test matrix

Everything in `src/utils/timing.rs` that can be tested without a browser is covered by
`cargo test --lib`. What cannot be tested there is the part that matters most: whether a
real browser, against a real CDN, gives the fields the table claims it does. Resource
Timing is the whole problem — its availability turns on `Timing-Allow-Origin`, its
buffer behaves differently per engine, and `cache: no-store` is honoured to different
depths.

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
| Live playlist (no `EXT-X-ENDLIST`) | Playlist rows must still run; media samples must not, and the live note must appear. |
| Encrypted VOD (`EXT-X-KEY`) | Bytes must be timed as opaque; no key request may appear. |
| Playlist with a `GAP` segment | The GAP must never be fetched. |
| A URL that 404s, and a cross-origin URL with no CORS headers | Rows must read "failed" with no duration. |

## Browsers

Run each check in Chrome/Edge (Blink), Safari (WebKit) and Firefox (Gecko), desktop at
minimum. Note the version tested.

## Checks

| # | Check | Expected |
| --- | --- | --- |
| 1 | Timing is unchecked on first load | Neither Timing item is selected; probing does no extra requests. |
| 2 | Playlist requests input | Accepts 1–20; typing 0 or 99 clamps to 1 or 20 on blur, and the run makes exactly that many. |
| 3 | Media pairs input | Accepts 1–10; clamps the same way. |
| 4 | Request count, master URL | 1 multivariant row + N media playlist rows. |
| 5 | Request count, media playlist URL | N media playlist rows, no multivariant row, and the "URL probed is a media playlist" note. With media timing on, the segment rows read "Media segment (muxing unknown)" — never "Muxed segment" — and the muxing note appears. |
| 6 | `no-store` | DevTools network panel shows no "(disk cache)" / "(memory cache)" for any Timing request, on repeat runs too. |
| 7 | No cache busting | Every Timing request URL is byte-identical to the playlist's URI: no added query parameters. |
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
| 23 | Live stream | Playlist rows present; no media rows; the "availability lag is not measured" note present; no `_HLS_msn` or `_HLS_part` parameter on any request. |
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
- Live availability lag is not measured at all yet.
