// Daily notes: where they live, finding them, and the calendar's date maths.
//
// With the "year-month" layout, a day's note is kept as
//   <daily folder>/2026/10 October/2026-10-24.md
// The file name is always the full date, so [[2026-10-24]] links never
// break and the notes sort correctly in any app.

export type DailyLayout = "year-month" | "flat";

// Folder names are fixed English, so they never change with the computer's language.
export const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

const DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

export const clean = (folder: string) => folder.replace(/^\/+|\/+$/g, "");

/** True for a real calendar date written as YYYY-MM-DD. */
export function isDay(s: string): boolean {
  const m = DAY.exec(s);
  if (!m) return false;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const dt = new Date(y, mo - 1, d);
  return dt.getFullYear() === y && dt.getMonth() === mo - 1 && dt.getDate() === d;
}

/** "10 October" for 2026-10-24. */
export function monthFolder(day: string): string {
  const mo = Number(day.slice(5, 7));
  return `${String(mo).padStart(2, "0")} ${MONTHS[mo - 1]}`;
}

/** The folder a day's note belongs in. */
export function dailyFolderFor(folder: string, day: string, layout: DailyLayout): string {
  const base = clean(folder);
  const parts = layout === "year-month" ? [base, day.slice(0, 4), monthFolder(day)] : [base];
  return parts.filter(Boolean).join("/");
}

/** Where a day's note belongs. */
export function dailyPathFor(folder: string, day: string, layout: DailyLayout): string {
  const dir = dailyFolderFor(folder, day, layout);
  return (dir ? `${dir}/` : "") + `${day}.md`;
}

const stem = (path: string) => path.slice(path.lastIndexOf("/") + 1).replace(/\.md$/i, "");

/** The day of a daily note: a note named by date anywhere in the daily folder. */
export function dayOf(path: string, folder: string): string | null {
  if (!/\.md$/i.test(path)) return null;
  const base = clean(folder);
  if (base && !path.startsWith(base + "/")) return null;
  const s = stem(path);
  return isDay(s) ? s : null;
}

/** Daily notes among `files`, oldest first, keyed by day (the first one wins a duplicate day). */
export function dailyIndex(files: string[], folder: string): Map<string, string> {
  const found = files.map((p) => [dayOf(p, folder), p] as const).filter((x): x is [string, string] => x[0] !== null);
  found.sort((a, b) => (a[0] === b[0] ? a[1].localeCompare(b[1]) : a[0] < b[0] ? -1 : 1));
  const map = new Map<string, string>();
  for (const [d, p] of found) if (!map.has(d)) map.set(d, p);
  return map;
}

/** Daily notes that aren't where the layout puts them: [path, folder it belongs in]. */
export function misplaced(files: string[], folder: string, layout: DailyLayout): [string, string][] {
  const out: [string, string][] = [];
  for (const [day, path] of dailyIndex(files, folder)) {
    const want = dailyPathFor(folder, day, layout);
    if (path !== want && !files.includes(want)) out.push([path, dailyFolderFor(folder, day, layout)]);
  }
  return out;
}

/** The same date in earlier years (Feb 29 also matches Feb 28 in other years), newest first. */
export function onThisDay(days: Iterable<string>, day: string): string[] {
  const md = day.slice(5);
  const year = day.slice(0, 4);
  const leap = md === "02-29";
  return [...days]
    .filter((d) => d.slice(0, 4) < year && (d.slice(5) === md || (leap && d.slice(5) === "02-28")))
    .sort()
    .reverse();
}

const pad = (n: number) => String(n).padStart(2, "0");
export const ymd = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`;

/** The first day of the week for the computer's language: 0 Sunday … 6 Saturday. */
export function weekStart(locale?: string): number {
  try {
    const l = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & { weekInfo?: { firstDay: number }; getWeekInfo?: () => { firstDay: number } };
    const first = l.getWeekInfo?.().firstDay ?? l.weekInfo?.firstDay;
    if (first) return first % 7;
  } catch {
    // Older systems: fall through.
  }
  return 0;
}

/** Six weeks of days covering month `m` (0-11) of year `y`, starting on `start`. */
export function monthGrid(y: number, m: number, start = 0): { day: string; inMonth: boolean }[] {
  const first = new Date(y, m, 1);
  const offset = (first.getDay() - start + 7) % 7;
  const out: { day: string; inMonth: boolean }[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(y, m, 1 - offset + i);
    out.push({ day: ymd(d.getFullYear(), d.getMonth(), d.getDate()), inMonth: d.getMonth() === m });
  }
  return out;
}
