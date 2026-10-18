# Third-party notices

Questkeep re-implements ideas, algorithms, file formats and registry paths from the following
projects. No source code is copied verbatim. Files containing derived logic carry the comment
`Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md`.

## Playnite and PlayniteExtensions

- https://github.com/JosefNemec/Playnite
- https://github.com/JosefNemec/PlayniteExtensions

```
MIT License

Copyright (c) 2020 Josef Nemec

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Other third-party dependencies (Rust crates, npm packages) are used under their own licenses as
declared in `Cargo.lock` and `pnpm-lock.yaml`.

## SteamGridDB

Questkeep can fetch cover and background images from the SteamGridDB API
(https://www.steamgriddb.com, API v2) when the user supplies their own free API key in the
settings. Artwork is community-uploaded and belongs to its authors; Questkeep only caches it
locally for personal use. SteamGridDB is not affiliated with Questkeep. The API terms were not
reachable while this was written, so check https://www.steamgriddb.com/api/v2 and the site's
terms before distributing builds, and add any attribution they require (none is shown in the UI
today).
