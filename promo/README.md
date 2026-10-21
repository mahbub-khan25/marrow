# Marrow promo video

A 64-second 1080p promo made with [Remotion](https://www.remotion.dev) from
screenshots of Marrow running on Windows (`public/*.jpg`).

```powershell
cd promo
npm install
npm run studio    # preview and tweak in the browser
npm run render    # writes out\marrow-promo.mp4
```

## Second promo (2K, 60 fps, 16:9 and 9:16)

`src/v2` is a 64-second promo drawn mostly in code: the problem, plain files,
live typing, speed, crash safety, features and a call to action. Every scene,
keystroke and sound cue is timed in `src/v2/timeline.json`.

```powershell
python audio/make_promo2_audio.py   # original music and effects -> public/promo2-audio.wav
npx remotion render src/index.ts MarrowPromo2Wide out/marrow-promo-16x9-2k60.mp4 --crf=16
npx remotion render src/index.ts MarrowPromo2Tall out/marrow-promo-9x16-2k60.mp4 --crf=16
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
