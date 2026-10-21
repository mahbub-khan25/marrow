# Marrow promo video

A 64-second 1080p promo made with [Remotion](https://www.remotion.dev) from
screenshots of Marrow running on Windows (`public/*.jpg`).

```powershell
cd promo
npm install
npm run studio    # preview and tweak in the browser
npm run render    # writes out\marrow-promo.mp4
```

## Sound

`public/music.wav` is an original ambient piano-and-pad track (made by
`audio/make_soundtrack.py`, free to use under the MIT license).

Run `python audio/make_soundtrack.py` before rendering. It writes
`public/soundtrack.wav`: the music, with a voiceover mixed on top if
`public/vo/<scene>.wav` files exist (one per scene, named as in `SCENES` in
the script). The music dips under each line.

The voiceover isn't included in this repository. Record your own, or use a
text-to-speech voice whose license allows commercial use. Many free voices
are trained on research-only recordings, so check the voice's model card
before publishing a video with it. If you change a scene's length in
`src/Promo.tsx`, change it in `SCENES` too.
