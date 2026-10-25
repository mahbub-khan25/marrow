// From Ekushe Next (https://github.com/mahbub-khan25/ekushenext-website, site/engine/ekushe.js),
// MPL-2.0. The engine itself is ui/public/ekushe/ekushe.wasm: ekushe-core built for the web.
// Ekushe Next typing engine, running in the browser (WebAssembly build of ekushe-core).
// Usage: const ek = await Ekushe.load(); ek.key('a') -> { preedit, candidates, committed, ... }
export class Ekushe {
  static async load(base = new URL('.', import.meta.url)) {
    const [wasm, dict] = await Promise.all([
      fetch(new URL('ekushe.wasm', base)).then((r) => r.arrayBuffer()),
      fetch(new URL('ekushe-web.dict.z', base)).then((r) => (r.ok ? r.arrayBuffer() : null)).catch(() => null),
    ]);
    const { instance } = await WebAssembly.instantiate(wasm, {});
    return new Ekushe(instance.exports, dict);
  }
  constructor(x, dict) {
    this.x = x;
    this.dec = new TextDecoder();
    let ptr = 0, len = 0;
    if (dict) {
      len = dict.byteLength;
      ptr = x.ek_alloc(len);
      new Uint8Array(x.memory.buffer, ptr, len).set(new Uint8Array(dict));
    }
    this.words = x.ek_init(ptr, len);
  }
  read() {
    const p = this.x.ek_out_ptr(), n = this.x.ek_out_len();
    return n ? JSON.parse(this.dec.decode(new Uint8Array(this.x.memory.buffer, p, n))) : {};
  }
  key(ch) { this.x.ek_key(ch.codePointAt(0)); return this.read(); }
  backspace() { this.x.ek_backspace(); return this.read(); }
  select(i, trailing = '') { this.x.ek_select(i, trailing ? trailing.codePointAt(0) : 0); return this.read(); }
  commit(trailing = '') { this.x.ek_commit(trailing ? trailing.codePointAt(0) : 0); return this.read(); }
  reset() { this.x.ek_reset(); return this.read(); }
  text(fn, str) {
    const bytes = new TextEncoder().encode(str);
    const p = this.x.ek_alloc(bytes.length || 1);
    new Uint8Array(this.x.memory.buffer, p, bytes.length).set(bytes);
    fn(p, bytes.length);
    this.x.ek_free(p, bytes.length || 1);
    const o = this.x.ek_out_ptr(), n = this.x.ek_out_len();
    return this.dec.decode(new Uint8Array(this.x.memory.buffer, o, n));
  }
  bijoyToUnicode(s) { return this.text(this.x.ek_bijoy_to_unicode, s); }
  unicodeToBijoy(s) { return this.text(this.x.ek_unicode_to_bijoy, s); }
}
