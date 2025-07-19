# hls-manifest-viewer

This site is uploaded to GitHub Pages here: https://therealrobg.github.io/hls-manifest-viewer/

This is a static site written almost entirely in Rust using the [Leptos][1] framework (compiled to
WebAssembly for serving). It makes use of my [m3u8][2] parsing library (also written in Rust).

The purpose of this site is to provide a tool that helps make browsing HLS manifests easier.
Sometimes, as a video streaming developer, you need to investigate issues that a player is having
and it can help to inspect the media (both playlists and media segments) it is receiving. With HLS
this can be an arduous process since the media segment locations are usually described using
multiple manifest files (one multivariant playlist and many media playlists). It can be frustrating
to need to download the multivariant, figure out the absolute URLs for the media playlists, and then
also download those. Add into the mix [HLS Variable Substitution][3] and this can get very boring
very quickly. This tools hopes to put everything needed to browse HLS media in a convenient user
interface that can be loaded in your web browser. Here are some of the goals of the tool:
* Provide resolved navigation links for media playlist URIs in the HLS playlist that don't require
  leaving the tool.
* Provide a view for inspecting [Media Segments][4] within the tool.
    * HLS has support for [MPEG-2 Transport Streams][5], [Fragmented MPEG-4][6], [Packed Audio][7],
      [WebVTT][8], and [IMSC Subtitles][9]. The highest priority for this tool is Fragmented MPEG-4
      support (mainly because it is what I deal with mostly, but also, it seems to be the defacto
      standard for streaming these days thanks to CMAF). WebVTT has basic support (given it is just
      text it is easy to dump that into a view). IMSC Subtitles can be seen as _partially_ supported
      since they are contained within Fragmented MPEG-4; however, there is a goal to provide better
      views into fMP4 contents (perhaps a view on CEA-608/708 captions, `id3` parsing within `emsg`
      atoms, etc.).
* Provide a view for parsing SCTE35 messages found in `EXT-X-DATERANGE` tags.
* Provide a view for visualizing what range of segments a given `EXT-X-DATERANGE` tag applies to
  (given that `EXT-X-DATERANGE` is described using [ISO_8601][10] and can appear anywhere in the
  playlist it isn't always easy to see if a segment is included in the range or not).
* Provide a view for JSON links from the manifest (such as for [Content Steering][11], for the
  [X-ASSET-LIST][12] attribute in HLS Interstitials, [EXT-X-SESSION-DATA][13], etc.).

So far only media playlist resolution, fMP4 (including range requests based on `EXT-X-BYTERANGE` /
`EXT-X-MAP:BYTERANGE`), and WebVTT views have been implemented.

But ultimately, this tool is just meant to be helpful in making working with HLS easier as a player
developer, and so there may be many other fun directions to go in. For example, an MSE player view
to see frames of content within a chosen Media Segment, or perhaps WebCodecs, or a validation tool
for the playlist (like [mediastreamvalidator][14])... Lots of ways to spend time here.

Also, I'm only working on this in my spare time, so a lot of these ideas may not get done if just
left to me. Based on this, I provide this with the [Unlicense license][15] so anyone is free to take
it and do what they want with it. If I'm really honest, I made this because I don't get to code much
these days but when I do I really like Rust, so I was searching for reasons to write something.
That's why this is written using Leptos rather than a more normal web framework (e.g. React, Vue, or
just plain old HTML+CSS+JavaScript).

[1]: https://leptos.dev
[2]: https://github.com/theRealRobG/m3u8
[3]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-4.3
[4]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1
[5]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.1
[6]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.2
[7]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.3
[8]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.4
[9]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-3.1.5
[10]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#ref-ISO_8601
[11]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-7.2
[12]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#appendix-D.2
[13]: https://datatracker.ietf.org/doc/html/draft-pantos-hls-rfc8216bis-17#section-4.4.6.4
[14]: https://developer.apple.com/documentation/http-live-streaming/using-apple-s-http-live-streaming-hls-tools
[15]: https://unlicense.org/

## Building Locally

### Prerequisites
* Rust (https://www.rust-lang.org/tools/install)
* `wasm32-unknown-unknown` target (https://doc.rust-lang.org/beta/rustc/platform-support/wasm32-unknown-unknown.html#building-rust-programs)
* Trunk (https://trunkrs.dev/#plain-cargo)

### Serve
In the root of the repository run:
```
trunk serve --public-url "/hls-manifest-viewer"
```
Annoyingly the `--public-url` flag is needed because I've hard-coded the GitHub Pages site path into
the project source code. GitHub Pages publishes the site to a path component from the base domain
(https://therealrobg.github.io/hls-manifest-viewer in this case). Trunk can fix source locations for
this by using this flag, which would hopefully only be needed when publishing; however, the Leptos
Router is broken by this unexpected path component. I've experimented with making this more dynamic,
as you can read the site's [Base URI][16] (from the Document), so theoretically I could update the
router paths for each page to be more dynamic on startup. I had a brief exploration here but the
paths defined in the router need to have static lifetimes so it's difficult to derive a path at
runtime (I _could_ do some nasty things like using [String::leak][17], but in the end, it was late
and I just wanted to get the site to work). This should be improved.

[16]: https://developer.mozilla.org/en-US/docs/Web/API/Node/baseURI
[17]: https://doc.rust-lang.org/std/string/struct.String.html#method.leak

### Release
The release process is handled by GitHub Actions ([pages.yml](.github/workflows/pages.yml)). But if
you want to build for release locally then run the following command:
```
trunk build --release --public-url "/hls-manifest-viewer"
```

## Acknowledgements
* The genesis of this tool (for me) is the brilliant [Adaptive Bitrate Manifest Viewer][18] Chrome
  extension, that sadly, is not available in the Chrome store anymore (as it does not follow best
  practices for Chrome extensions and the developer does not have the time to update it). This
  extension would hijack any requests to URLs with file extension `.m3u8` or `.m3u` and reload it
  in a new page it would deliver instead that would make the manifest request, parse it, and provide
  a colorized output with links that actually resolve properly. For me, this made my early learning
  of HLS **much** easier, as I could easily browse manifests that I was dealing with and quickly
  cross-reference tags against the HLS spec (which is also very convenient in the IETF site that it
  has a "html-ized" version with quick links to the various sections). I still feel that this tool
  is more convenient than my one, as it would automatically handle requests from search in the
  Chrome URL input bar, so one less degree of separation and super convenient. I may later look into
  delivering a Chrome extension too for the convenience factor (I believe Chrome extensions support
  WebAssembly).
* The fantastic [MP4Box.js / ISOBMFF Box Structure Viewer][19] is where most of the inspiration for
  the "isobmff" view came from. That site does a really great job in displaying the contents of MP4
  files, and at some stage I may add a button to link directly to that site for the fMP4 (I've found
  that this works just by including the file URL as the query string, i.e., no key, just the value).
  The only benefit for displaying the view in my tool is that it keeps the investigation of the
  media in one place without having to jump around too many tools (and also, it was educational for
  me to go through all of the ISOBMFF defined boxes, and further boxes e.g. CENC, AVC1, etc.).
* Professionally, working with the [Comcast/mamba][20] (especially when writing many different types
  of manifest manipulation for iOS playback) gave me my familiarity with the concepts of HLS, and
  also demonstrated to me how zero copy parsers can be very fast (when a naiive younger me tried to
  re-write it in Swift using plenty of String allocations and finding it orders of magnitudes slower
  than mamba). As a result, I had a desire for some time to write a "zero-copy" HLS parser in Rust,
  and eventually found the time to write [m3u8][2].
* The great [PSSH box tools][21] GitHub Pages site finally gave me the idea that I could write a
  GitHub Pages site with Rust that could provide a collection of tools to help me (and others) do
  what I (/we) do easier.

[18]: https://chromewebstore.google.com/detail/adaptive-bitrate-manifest/omjpjjekjefmdkidigpkhpjnojoadbih?hl=en
[19]: https://dist.gpac.io/mp4box.js/test/filereader.html
[20]: https://github.com/Comcast/mamba
[21]: https://emarsden.github.io/pssh-box-wasm/
