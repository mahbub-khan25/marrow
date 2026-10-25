export interface EkCandidate {
  text: string;
  kind: string;
  annotation: string | null;
}
export interface EkOutput {
  language?: string;
  preedit?: string;
  roman_buffer?: string;
  candidates?: EkCandidate[];
  selected_index?: number;
  next_word_predictions?: string[];
  committed?: string | null;
  delete_before?: number;
}
export class Ekushe {
  static load(base?: URL): Promise<Ekushe>;
  constructor(exports: WebAssembly.Exports, dict: ArrayBuffer | null);
  words: number;
  key(ch: string): EkOutput;
  backspace(): EkOutput;
  select(i: number, trailing?: string): EkOutput;
  commit(trailing?: string): EkOutput;
  reset(): EkOutput;
  bijoyToUnicode(s: string): string;
  unicodeToBijoy(s: string): string;
}
