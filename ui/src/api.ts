// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type EntryKind = "File" | "Folder";

export interface TreeNode {
  path: string;
  name: string;
  kind: EntryKind;
  children: TreeNode[];
}

export interface IndexStats {
  unchanged: number;
  reused: number;
  parsed: number;
}

export interface VaultInfo {
  root: string;
  name: string;
  notes: number;
  stats: IndexStats;
  /** What opening it repaired after a crash. */
  recovered?: string[];
}

export interface BacklinkGroup {
  source: string;
  title: string;
  items: { line: number; ch: number; snippet: string }[];
}

export interface NoteLinks {
  outgoing: string[];
  unresolved: string[];
}

export interface FileHit {
  path: string;
  name: string;
  folder: string;
  isNote: boolean;
  linkText: string;
}

export interface Backup {
  vault: string;
  path: string;
  text: string;
  /** Milliseconds since the Unix epoch. */
  savedAt: number;
}

export interface SearchResults {
  hits: {
    path: string;
    name: string;
    matches: number;
    snippets: { line: number; text: string; ranges: [number, number][]; ch: number; len: number }[];
  }[];
  total: number;
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface RenameReport {
  newPath: string;
  linksUpdated: number;
  filesUpdated: string[];
  /** Links left alone because the note changed during the rename. */
  linksSkipped?: number;
}

export interface GraphNode {
  id: string;
  name: string;
  kind: "note" | "attachment" | "unresolved" | "tag";
  degree: number;
  tags: string[];
}

export interface GraphData {
  nodes: GraphNode[];
  edges: { source: string; target: string }[];
}

export interface NoteInfo {
  /** With `#`, first-seen order. */
  tags: string[];
  modifiedMs: number;
  backlinks: number;
}

export interface VaultStats {
  notes: number;
  links: number;
}

export interface RecentFile {
  path: string;
  modifiedMs: number;
}

export type LinkFormat = "shortest" | "relative" | "absolute";

export interface VaultSettings {
  linkFormat: LinkFormat;
  /** "root", "current" (the open note's folder) or "folder". */
  newNoteLocation: "root" | "current" | "folder";
  newNoteFolder: string;
  dailyFolder: string;
  /** "year-month": Daily/2026/10 October/2026-10-24.md. "flat": all in the daily folder. */
  dailyLayout?: "year-month" | "flat";
  templateFolder: string;
  /** Starred notes, shown at the top of the file tree. */
  starred: string[];
  /** CSS snippets that are on. */
  snippets: string[];
  /** Plugins that are on. */
  plugins: string[];
  /** Property name → the type picked for it. */
  propertyTypes?: Record<string, string>;
}

export interface PluginInfo {
  dir: string;
  manifest: { id?: string; name?: string; version?: string; description?: string; author?: string; permissions?: string[] } | null;
  code: string | null;
  problem: string | null;
}

/** A to-do somewhere in the vault. */
export interface TaskItem {
  path: string;
  name: string;
  line: number;
  done: boolean;
  text: string;
  /** YYYY-MM-DD */
  due: string | null;
  /** The date comes from the daily note the task is in. */
  fromNote: boolean;
}

/** A Notion or Evernote export found in Downloads. */
export interface FoundExport {
  path: string;
  name: string;
  size: number;
  modifiedMs: number;
}

/** A note as it was at some moment (the time machine). */
export interface ThenNote {
  path: string;
  state: "same" | "changed" | "gone";
  /** The kept version holding it then; null: the note as it is now. */
  version: number | null;
}

/** An earlier version of a note. */
export interface NoteVersion {
  timeMs: number;
  size: number;
}

/** A plain-text mention of a note in another note. */
export interface Mention {
  path: string;
  name: string;
  line: number;
  text: string;
  /** UTF-16 column and length of the mention in `text`. */
  ch: number;
  len: number;
  matched: string;
}

export interface GraphOptions {
  showUnresolved: boolean;
  showAttachments: boolean;
  showTags: boolean;
}

export type VaultEvent =
  | { type: "created"; path: string; kind: EntryKind }
  | { type: "modified"; path: string }
  | { type: "deleted"; path: string; kind: EntryKind }
  | { type: "renamed"; from: string; to: string; kind: EntryKind };

export type EngineEvent =
  | { type: "vault"; event: VaultEvent }
  | { type: "metadata-changed"; path: string }
  | { type: "links-changed"; paths: string[] };

export interface LockInfo {
  /** The file on disk is encrypted. */
  locked: boolean;
  /** Its password (or its folder's) was entered this session. */
  unlocked: boolean;
  hint: string | null;
  /** The locked folder it's in, or is. */
  folder: string | null;
}

let askUnlock: ((folder: string) => Promise<boolean>) | null = null;

/** How to ask for a locked folder's password when an action needs it. */
export function setUnlockHandler(f: (folder: string) => Promise<boolean>) {
  askUnlock = f;
}

/**
 * Runs an action that may put a note into a locked folder. If the folder's
 * password hasn't been entered, asks for it and tries once more.
 */
async function intoLockedFolder<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (e) {
    const s = String(e);
    if (!s.startsWith("LOCKED_FOLDER:")) throw e;
    const folder = s.slice("LOCKED_FOLDER:".length);
    if (askUnlock && (await askUnlock(folder))) return run();
    throw new Error(`“${folder || "This vault"}” is locked. Enter its password to put notes there.`);
  }
}

export const api = {
  lockInfo: (path: string) => invoke<LockInfo>("lock_info", { path }),
  lockedPaths: () => invoke<{ notes: string[]; folders: string[] }>("locked_paths"),
  unlock: (path: string, password: string) => invoke<void>("unlock", { path, password }),
  lockNote: (path: string, password: string, hint: string | null) => invoke<void>("lock_note", { path, password, hint }),
  lockFolder: (path: string, password: string, hint: string | null) => invoke<number>("lock_folder", { path, password, hint }),
  removeLock: (path: string) => invoke<number>("remove_lock", { path }),
  lockNow: () => invoke<void>("lock_now"),
  extrasDir: (kind: "snippets" | "plugins") => invoke<string>("extras_dir", { kind }),
  snippetsList: () => invoke<{ name: string; css: string }[]>("snippets_list"),
  pluginsList: () => invoke<PluginInfo[]>("plugins_list"),
  extrasWrite: (kind: "snippets" | "plugins", rel: string, contents: string) => invoke<boolean>("extras_write", { kind, rel, contents }),
  siteBegin: (out: string) => invoke<void>("site_begin", { out }),
  siteWrite: (out: string, rel: string, contents: string) => invoke<void>("site_write", { out, rel, contents }),
  siteCopy: (out: string, rel: string, path: string) => invoke<void>("site_copy", { out, rel, path }),
  siteFinish: (out: string, files: string[]) => invoke<number>("site_finish", { out, files }),
  siteOpen: (out: string) => invoke<void>("site_open", { out }),
  lastVault: () => invoke<string | null>("last_vault"),
  /** Which kind of device this is ("ios", "android", "windows"…). */
  appPlatform: () => invoke<{ os: string; mobile: boolean }>("app_platform"),
  /** Phones and tablets: the app's own "Marrow" notes folder, made if needed. */
  defaultVault: () => invoke<string>("default_vault"),
  openVault: (path: string) => invoke<VaultInfo>("open_vault", { path }),
  tree: () => invoke<TreeNode>("tree"),
  readNote: (path: string) => invoke<string>("read_note", { path }),
  /** Saves `text`. With `expected`, refuses (error "CHANGED_ON_DISK") if the file no longer holds it. */
  writeNote: (path: string, text: string, expected?: string) =>
    invoke<void>("write_note", { path, text, expected: expected ?? null }),
  mergeText: (base: string, mine: string, theirs: string) =>
    invoke<{ text: string; conflicts: number }>("merge_text", { base, mine, theirs }),
  backupFor: (path: string) => invoke<Backup | null>("backup_for", { path }),
  discardBackup: (path: string) => invoke<void>("discard_backup", { path }),
  /** Keeps unsaved text as a crash draft (offered back like a backup). */
  keepDraft: (path: string, text: string) => invoke<void>("keep_draft", { path, text }),
  openLogFolder: () => invoke<void>("open_log_folder"),
  createNote: (folder: string, name?: string) => invoke<string>("create_note", { folder, name }),
  createFolder: (parent: string) => invoke<string>("create_folder", { parent }),
  trash: (path: string) => invoke<void>("trash", { path }),
  rename: (path: string, newName: string) => intoLockedFolder(() => invoke<RenameReport>("rename", { path, newName })),
  movePath: (path: string, folder: string) => intoLockedFolder(() => invoke<RenameReport>("move_path", { path, folder })),
  graph: (options: GraphOptions) => invoke<GraphData>("graph", { options }),
  search: (query: string, limit = 100) => invoke<SearchResults>("search", { query, limit }),
  tags: () => invoke<TagCount[]>("tags"),
  backlinks: (path: string) => invoke<BacklinkGroup[]>("backlinks", { path }),
  noteLinks: (path: string) => invoke<NoteLinks>("note_links", { path }),
  tasks: (includeDone: boolean) => invoke<TaskItem[]>("tasks", { includeDone }),
  /** The system-wide quick-capture shortcut ("CommandOrControl+Alt+N"), or null for none. */
  setCaptureShortcut: (shortcut: string | null) => invoke<void>("set_capture_shortcut", { shortcut }),
  historyList: (path: string) => invoke<NoteVersion[]>("history_list", { path }),
  historyRead: (path: string, timeMs: number) => invoke<string>("history_read", { path, timeMs }),
  /** Keeps `text` as a version of the note now. */
  historyVaultAt: (timeMs: number) => invoke<ThenNote[]>("history_vault_at", { timeMs }),
  historyTimeline: () => invoke<{ days: [number, number][]; firstMs: number | null }>("history_timeline"),
  historySnapshot: (path: string, text: string) => invoke<void>("history_snapshot", { path, text }),
  unlinkedMentions: (path: string, names: string[]) => invoke<Mention[]>("unlinked_mentions", { path, names }),
  resolveLink: (link: string, source: string) => invoke<string | null>("resolve_link", { link, source }),
  followLink: (link: string, source: string) => invoke<string>("follow_link", { link, source }),
  findFiles: (query: string, source?: string, limit = 30) =>
    invoke<FileHit[]>("find_files", { query, source: source ?? null, limit }),
  noteInfo: (path: string) => invoke<NoteInfo | null>("note_info", { path }),
  vaultStats: () => invoke<VaultStats>("vault_stats"),
  recentFiles: (limit = 20) => invoke<RecentFile[]>("recent_files", { limit }),
  /** Creates `name.ext` (made unique) in `folder` with `contents`. */
  createFile: (folder: string, name: string, ext: string, contents: string) =>
    intoLockedFolder(() => invoke<string>("create_file", { folder, name, ext, contents })),
  /** Copies a file from the computer into `folder` (or returns its vault path if it's already in the vault). */
  importFile: (source: string, folder: string) => invoke<string>("import_file", { source, folder }),
  /** Saves pasted or dropped bytes as a file in `folder`; returns its vault path. */
  /** The AI service that can turn voice notes into text, if one is set up. */
  aiTranscriber: () => invoke<string | null>("ai_transcriber"),
  /** A voice note as text, from that service (null when none is set up). */
  aiTranscribe: (bytes: Uint8Array, name: string) => invoke<string | null>("ai_transcribe", bytes, { headers: { name: encodeURIComponent(name) } }),
  saveAttachment: (bytes: Uint8Array, name: string, folder: string) =>
    invoke<string>("save_attachment", bytes, { headers: { name: encodeURIComponent(name), folder: encodeURIComponent(folder) } }),
  /** Saves the clipboard's image as a PNG (when the webview doesn't pass it on). */
  saveClipboardImage: (name: string, folder: string) => invoke<string | null>("save_clipboard_image", { name, folder }),
  /** Converts picked files (Notion zip, .enex, .docx, .html, .md) into a new vault folder. */
  /** Recent Notion or Evernote exports in Downloads. */
  findExports: (kind: "notion" | "evernote") => invoke<FoundExport[]>("find_exports", { kind }),
  importFiles: (kind: string, paths: string[], folder: string) =>
    invoke<{ folder: string; notes: number; attachments: number; warnings: string[] }>("import_files", { kind, paths, folder }),
  /** Writes an exported file anywhere (the person picked the place). */
  writeExport: (path: string, contents: string) => invoke<void>("write_export", { path, contents }),
  /** A folder's notes with their properties, for database views. */
  queryNotes: (folder: string, recursive = false) => invoke<QueryResult>("query_notes", { folder, recursive }),
  aiConfig: () => invoke<AiConfig>("ai_config"),
  /** `key`: a new key, "" to remove it, or undefined to keep the saved one. */
  aiSaveProvider: (provider: AiProvider, key?: string) => invoke<AiConfig>("ai_save_provider", { provider, key }),
  aiRemoveProvider: (id: string) => invoke<AiConfig>("ai_remove_provider", { id }),
  aiSetDefault: (id: string) => invoke<AiConfig>("ai_set_default", { id }),
  aiModels: (provider: AiProvider, key?: string) => invoke<string[]>("ai_models", { provider, key }),
  aiTest: (provider: AiProvider, key?: string) => invoke<string>("ai_test", { provider, key }),
  /** Starts a request; returns the provider's name. Text arrives through onAiEvent. */
  aiRun: (id: string, system: string, messages: AiMessage[], providerId?: string) =>
    invoke<string>("ai_run", { id, providerId, system, messages }),
  aiCancel: (id: string) => invoke<void>("ai_cancel", { id }),
  aiIndexInfo: () => invoke<IndexInfo>("ai_index_info"),
  aiSetEmbedding: (providerId: string | null, model: string | null) => invoke<void>("ai_set_embedding", { providerId, model }),
  aiIndexBuild: () => invoke<void>("ai_index_build"),
  /** The parts of notes that best match `query` (words, and meaning when set up). */
  aiRecall: (query: string, k = 8, mode: "both" | "meaning" = "both") => invoke<Recall>("ai_recall", { query, k, mode }),
  /** Dropbox, OneDrive, Google Drive and iCloud folders on this computer. */
  cloudLocations: () => invoke<CloudLocation[]>("cloud_locations"),
  /** Makes an empty vault folder `name` in `parent`; returns its path. */
  createVault: (parent: string, name: string) => invoke<string>("create_vault", { parent, name }),
  /** The cloud folder the open vault lives in, if any. */
  vaultCloud: () => invoke<CloudLocation | null>("vault_cloud"),
  mirrorStatus: () => invoke<MirrorStatus | null>("mirror_status"),
  /** How many files a folder already holds. */
  mirrorPreview: (remote: string) => invoke<number>("mirror_preview", { remote }),
  /** Keeps the vault in two-way sync with `remote`, or stops with null. */
  /** Syncs with a folder (end-to-end encrypted with `passphrase`), or turns sync off with null. */
  setMirror: (remote: string | null, passphrase?: string) => invoke<void>("set_mirror", { remote, passphrase: passphrase ?? null }),
  /** Whether a folder holds a vault Marrow encrypted. */
  folderEncrypted: (remote: string) => invoke<boolean>("folder_encrypted", { remote }),
  /** Signs in to a WebDAV server and says how many files its sync folder has. */
  webdavCheck: (url: string, user: string, password: string) => invoke<{ files: number; encrypted: boolean }>("webdav_check", { url, user, password }),
  /** Syncs this vault with a WebDAV server from now on. */
  setCloudSync: (url: string, user: string, password: string, passphrase?: string) =>
    invoke<void>("set_cloud_sync", { url, user, password, passphrase: passphrase ?? null }),
  syncNow: (force = false) => invoke<void>("sync_now", { force }),
  conflictCopies: () => invoke<ConflictCopy[]>("conflict_copies"),
  /** Merges a conflict copy into its original; returns places where both versions were kept. */
  resolveConflict: (copy: string, original: string) => invoke<number>("resolve_conflict", { copy, original }),
  getVaultSettings: () => invoke<VaultSettings>("get_vault_settings"),
  setVaultSettings: (settings: VaultSettings) => invoke<void>("set_vault_settings", { settings }),
};

export interface NoteRow {
  path: string;
  title: string;
  props: Record<string, unknown>;
  tags: string[];
  modifiedMs: number;
  createdMs: number;
  /** For each property holding [[links]], the notes they point to. */
  propLinks: Record<string, string[]>;
  backlinks: string[];
}

export interface QueryResult {
  rows: NoteRow[];
  related: NoteRow[];
}

export interface AiProvider {
  id: string;
  preset: string;
  name: string;
  protocol: "openai" | "anthropic";
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

export interface AiConfig {
  providers: AiProvider[];
  defaultId: string | null;
  /** The model for search by meaning. */
  embed: { providerId: string; model: string } | null;
}

export interface IndexInfo {
  providerId: string | null;
  model: string | null;
  /** Parts the notes are cut into. */
  chunks: number;
  /** Of those, how many have an embedding. */
  indexed: number;
  building: boolean;
}

export interface RecallHit {
  path: string;
  name: string;
  line: number;
  heading: string;
  text: string;
  how: "words" | "meaning" | "both";
}

export interface Recall {
  hits: RecallHit[];
  meaning: boolean;
  note: string | null;
}

export interface AiMessage {
  role: "user" | "assistant";
  content: string;
}

export interface CloudLocation {
  provider: "dropbox" | "onedrive" | "gdrive" | "icloud";
  label: string;
  path: string;
}

export type SyncStop =
  | { kind: "missing" }
  | { kind: "offline"; message: string }
  | { kind: "signIn"; message: string }
  | { kind: "looksEmpty"; side: "local" | "remote" }
  | { kind: "manyDeletes"; side: "local" | "remote"; count: number }
  | { kind: "io"; message: string };

export interface MirrorStatus {
  remote: string;
  label: string;
  provider: CloudLocation["provider"] | "webdav" | null;
  lastSync: number | null;
  syncing: boolean;
  stop: SyncStop | null;
  message: string | null;
}

export interface SyncReport {
  toLocal: string[];
  toRemote: string[];
  deletedLocal: string[];
  deletedRemote: string[];
  merged: string[];
  conflictCopies: string[];
  errors: string[];
}

export interface ConflictCopy {
  copy: string;
  original: string;
  source: "dropbox" | "onedrive" | "gdrive" | "syncthing" | "marrow";
}

export interface AiEvent {
  id: string;
  kind: "chunk" | "done" | "error";
  text: string;
}

export function onLocksChanged(cb: () => void): Promise<UnlistenFn> {
  return listen("locks-changed", () => cb());
}

export function onAiEvent(cb: (e: AiEvent) => void): Promise<UnlistenFn> {
  return listen<AiEvent>("ai-event", (e) => cb(e.payload));
}

export function onAiIndex(cb: (e: { done?: number; total?: number; finished?: boolean; error?: string }) => void): Promise<UnlistenFn> {
  return listen<{ done?: number; total?: number; finished?: boolean; error?: string }>("ai-index", (e) => cb(e.payload));
}

export function onMirrorChanged(cb: (status: MirrorStatus, report: SyncReport | null) => void): Promise<UnlistenFn> {
  return listen<{ status: MirrorStatus; report: SyncReport | null }>("mirror-changed", (e) => cb(e.payload.status, e.payload.report));
}

export function onVaultChanged(cb: (events: EngineEvent[]) => void): Promise<UnlistenFn> {
  return listen<EngineEvent[]>("vault-changed", (e) => cb(e.payload));
}

/** `folder/Note.md` → `Note`; `Map.canvas` → `Map`; `img/a.png` → `a.png`. */
export function displayName(path: string): string {
  const name = path.split("/").pop() ?? path;
  const lower = name.toLowerCase();
  if (lower.endsWith(".md")) return name.slice(0, -3);
  if (lower.endsWith(".canvas")) return name.slice(0, -7);
  return name;
}

export const isNotePath = (p: string) => p.toLowerCase().endsWith(".md");
export const isCanvasPath = (p: string) => p.toLowerCase().endsWith(".canvas");

export function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}
