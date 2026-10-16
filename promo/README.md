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

`public/soundtrack.wav` is the finished mix: an original ambient piano-and-pad
track with the voiceover on top, the music dipping under each line.

* `public/vo/*.wav` are the voiceover lines, one per scene, read by the free
  Piper voice *en_US-lessac-high*. To change a line, regenerate it:

  ```powershell
  pip install piper-tts
  # download en_US-lessac-high.onnx and .onnx.json from huggingface.co/rhasspy/piper-voices
  echo "Your new line." | python -m piper -m en_US-lessac-high.onnx -f public\vo\write.wav
  ```

* `python audio/make_soundtrack.py` rebuilds the music (`public/music.wav`)
  and the mix. If you change a scene's length in `src/Promo.tsx`, change it in
  `SCENES` in that script too, so each line still starts with its scene.
