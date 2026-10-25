# Licence for the Ekushe Next dictionary data

The dictionary data in this folder is licensed under
**Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)** ([full text](CC-BY-SA-4.0.txt)).
That covers `ekushe.dict.z`, `lexicon.tsv`, `review-spelling.tsv`, `roman/` and every file `tools/dict-builder`
generates. The code that builds and reads it (`tools/dict-builder`, `crates/ekushe-core`) is MPL-2.0, like
the rest of the project.

The data contains word-frequency counts, spelling classifications and word pairs derived from these sources.
No source text is included.

| Source | Licence |
| --- | --- |
| Bangla Wikipedia (via `wikimedia/wikipedia`, snapshot 2023-11-01) | CC BY-SA 3.0 / GFDL |
| FineWeb-2 `ben_Beng` by Hugging Face | ODC-By 1.0 (subject to Common Crawl terms of use) |
| Sangraha (verified Bengali) by AI4Bharat | CC BY 4.0 |
| wordfreq English word list (loanword matching only) | CC BY-SA 4.0 |

ShareAlike is used because Wikipedia and wordfreq are ShareAlike sources. If you redistribute or adapt this
data, keep this notice, credit the sources above, and license your version under CC BY-SA 4.0 (or a
compatible licence).

**Suggested attribution:** "Ekushe Next dictionary (CC BY-SA 4.0), derived from Bangla Wikipedia
(CC BY-SA), FineWeb-2 by Hugging Face (ODC-By), Sangraha by AI4Bharat (CC BY 4.0) and wordfreq (CC BY-SA 4.0)."
