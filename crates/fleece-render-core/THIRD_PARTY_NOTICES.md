# Third-Party Notices

## Rust renderer references

- [skinview3d v3.4.2](https://github.com/bs-community/skinview3d/tree/v3.4.2): the renderer adapts player-model geometry and box UV mapping from [`src/model.ts`](https://github.com/bs-community/skinview3d/blob/v3.4.2/src/model.ts). It does not ship skinview3d or Three.js as runtime dependencies. The font asset is credited separately below.
- [skinview-utils v0.7.1](https://github.com/bs-community/skinview-utils/tree/v0.7.1): the Rust texture code adapts the slim-model detection, legacy skin conversion, and opaque-layer handling from [`src/process.ts`](https://github.com/bs-community/skinview-utils/blob/v0.7.1/src/process.ts).

The two tagged releases contain identical MIT license notices, reproduced once here:

```text
MIT License

Copyright (c) 2014-2018 Kent Rasmussen
Copyright (c) 2017-2022 Haowei Wen, Sean Boult and contributors

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

## Minecraft name-tag font

The embedded `minecraft.ttf` is **Minecraft** by **Pwnage_Block**, copyright 2011. The [original FontStruct page](https://fontstruct.com/fontstructions/show/432966/minecraft_14) and the font's metadata both identify the font license as [Creative Commons Attribution-ShareAlike 3.0 Unported](https://creativecommons.org/licenses/by-sa/3.0/).

[skinview3d's `assets/minecraft.woff2` at v3.4.2](https://github.com/bs-community/skinview3d/blob/v3.4.2/assets/minecraft.woff2) is a distribution copy, not the original publication. When decoded to TrueType, that copy matches the embedded file byte-for-byte except for the modification timestamp and the two affected checksum fields. The WOFF2-to-TTF format conversion did not change the glyphs.

[South-Paw/typeface-minecraft](https://github.com/South-Paw/typeface-minecraft) also distributes webfont variants of the same typeface. Its WOFF2 differs from the skinview3d copy and is not identified here as the source of the embedded file. The MIT license of that repository is not the font license.

The full font license, `LICENSE-minecraft-font-CC-BY-SA-3.0.txt`, is included under `crates/fleece-render-core/assets/` in the repository, `assets/` in the core Cargo package, and the container's documentation directory.

## Compatibility references

- [Plain Craft Launcher 2 (PCL)](https://github.com/Meloong-Git/PCL), by 龙腾猫跃 (LTCat) and contributors: the 2D avatar uses its face/hat layering and adapts the hat visibility check from [`MySkin.xaml.vb`](https://github.com/Meloong-Git/PCL/blob/93ede5d9e638c99fcec853a9ba1eecd91ceee7f5/Plain%20Craft%20Launcher%202/Pages/PageLaunch/MySkin.xaml.vb). No PCL binaries, UI files, or assets are bundled. See the [upstream notice and attribution guidance](https://github.com/Meloong-Git/PCL/blob/93ede5d9e638c99fcec853a9ba1eecd91ceee7f5/LICENCE). fleece-render is not affiliated with PCL.
- [SerinaNya/SkinRenderMC](https://github.com/SerinaNya/SkinRenderMC) is an inspiration and an HTTP API compatibility reference, not a bundled dependency.

fleece-render is not an official Minecraft product and is not approved by or associated with Mojang or Microsoft.
