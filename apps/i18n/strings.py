#!/usr/bin/env python3
"""The phone apps' words, in English and Bangla, in one place.

    python3 apps/i18n/strings.py

writes the Android resources (values/strings.xml, values-bn/strings.xml)
and the iOS string catalog (Localizable.xcstrings). Formats use Android's
style here (%1$s, %1$d); the iOS catalog gets %@ and %lld.

The Bangla follows the desktop app's (ui/src/i18n-bn.ts), so a word means
the same on every device.
"""

import json
import os
import re
from xml.sax.saxutils import escape

HERE = os.path.dirname(os.path.abspath(__file__))
APPS = os.path.dirname(HERE)

# key, English, Bangla
STRINGS = [
    ("app_name", "Marrow", "ম্যারো"),
    ("share_to_marrow", "Add to Marrow", "ম্যারোতে যোগ করুন"),
    ("today", "Today", "আজ"),
    ("notes", "Notes", "নোট"),
    ("ask", "Ask", "জিজ্ঞাসা"),
    ("tasks", "Tasks", "কাজ"),
    ("search", "Search", "খুঁজুন"),
    ("settings", "Settings", "সেটিংস"),
    ("back", "Back", "পেছনে"),
    ("more", "More", "আরও"),
    ("ok", "OK", "ঠিক আছে"),
    ("cancel", "Cancel", "বাতিল"),
    ("save", "Save", "সংরক্ষণ"),
    ("done", "Done", "হয়ে গেছে"),
    ("open", "Open", "খুলুন"),
    ("hide_sidebar", "Hide the sidebar", "সাইডবার লুকান"),
    ("show_sidebar", "Show the sidebar", "সাইডবার দেখান"),
    ("edit", "Edit", "সম্পাদনা"),
    ("delete", "Delete", "মুছুন"),
    ("rename", "Rename", "নাম বদলান"),
    ("create", "Create", "তৈরি করুন"),
    ("remove", "Remove", "সরান"),
    ("clear", "Clear", "মুছে ফেলুন"),
    ("copy", "Copy", "কপি"),
    ("copied", "Copied", "কপি হয়েছে"),
    ("share", "Share", "শেয়ার করুন"),
    ("send", "Send", "পাঠান"),
    ("stop", "Stop", "থামান"),
    ("check", "Check", "যাচাই করুন"),
    ("test", "Test", "পরীক্ষা"),
    ("list", "List", "তালিকা"),
    ("name", "Name", "নাম"),
    ("continue_", "Continue", "চালিয়ে যান"),
    ("untitled", "Untitled", "শিরোনামহীন"),
    ("photo", "Photo", "ছবি"),
    ("locked", "Locked", "লক করা"),
    ("one_item", "1 item", "১টি জিনিস"),
    ("n_items", "%1$d items", "%1$dটি জিনিস"),
    ("recent", "Recent", "সাম্প্রতিক"),
    ("just_now", "Just now", "এইমাত্র"),
    ("see_all", "See all", "সব দেখুন"),
    ("calendar", "Calendar", "ক্যালেন্ডার"),
    ("graph", "Graph", "গ্রাফ"),
    ("tags", "Tags", "ট্যাগ"),
    ("time_machine", "Time machine", "টাইম মেশিন"),
    ("starred", "Starred", "তারকাচিহ্নিত"),
    ("star", "Star", "তারকা দিন"),
    ("unstar", "Remove star", "তারকা সরান"),
    ("all_notes", "All notes", "সব নোট"),
    ("sort", "Sort", "সাজান"),
    ("sort_name", "By name", "নাম অনুযায়ী"),
    ("sort_modified", "Last changed first", "সর্বশেষ বদলানো আগে"),
    ("new_", "New", "নতুন"),
    ("new_note", "New note", "নতুন নোট"),
    ("new_folder", "New folder", "নতুন ফোল্ডার"),
    ("new_canvas", "New canvas", "নতুন ক্যানভাস"),
    ("from_template", "From a template", "টেমপ্লেট থেকে"),
    ("move_to", "Move to…", "সরান…"),
    ("move_name_to", "Move “%1$s” to", "“%1$s” সরান যেখানে"),
    ("top_of_vault", "Top of the vault", "ভল্টের শুরুতে"),
    ("duplicate", "Duplicate", "অনুলিপি করুন"),
    ("empty_folder", "Nothing here yet", "এখানে এখনো কিছু নেই"),
    ("empty_folder_text", "Tap New to write a note, or make a folder to keep notes together.", "নোট লিখতে নতুন-এ চাপুন, বা নোট একসাথে রাখতে ফোল্ডার বানান।"),
    ("delete_note_q", "Delete “%1$s”?", "“%1$s” মুছবেন?"),
    ("delete_note_text", "It goes to the vault's trash folder (.marrow/trash), so you can still get it back.", "এটি ভল্টের ট্র্যাশ ফোল্ডারে (.marrow/trash) যাবে, তাই পরে ফিরিয়ে আনা যাবে।"),
    ("delete_folder_text", "The folder and everything in it go to the vault's trash folder (.marrow/trash).", "ফোল্ডারটি ও এর ভেতরের সবকিছু ভল্টের ট্র্যাশ ফোল্ডারে (.marrow/trash) যাবে।"),
    ("no_notes_with_tag", "No notes carry this tag any more.", "এই ট্যাগ এখন আর কোনো নোটে নেই।"),
    # Today
    ("whats_on_your_mind", "What's on your mind?", "মনে কী আছে?"),
    ("what_needs_doing", "What needs doing?", "কী করতে হবে?"),
    ("quick_note", "Quick note", "দ্রুত নোট"),
    ("todays_note", "Today's note", "আজকের নোট"),
    ("start_todays_note", "Start today's note", "আজকের নোট শুরু করুন"),
    ("todays_note_hint", "A place for the day's thoughts and to-dos.", "দিনের ভাবনা ও করণীয় লেখার জায়গা।"),
    ("add_task", "Add a task", "একটি কাজ যোগ করুন"),
    ("add_photo", "Add a photo", "ছবি যোগ করুন"),
    ("dictate", "Dictate", "বলে লিখুন"),
    ("no_dictation", "This phone has no voice typing.", "এই ফোনে কণ্ঠে লেখার ব্যবস্থা নেই।"),
    ("as_a_task", "As a task", "কাজ হিসেবে"),
    ("added_to_today", "Added to today's note.", "আজকের নোটে যোগ হয়েছে।"),
    ("added_to_tasks", "Added to today's tasks.", "আজকের কাজে যোগ হয়েছে।"),
    ("due_today_and_overdue", "Due today and overdue", "আজকের ও পেরিয়ে যাওয়া কাজ"),
    ("worth_another_look", "Worth another look", "আরেকবার দেখার মতো"),
    ("from_your_archive", "From your archive", "আপনার সংগ্রহ থেকে"),
    ("not_opened_since", "Last opened %1$s", "শেষ খোলা হয়েছে %1$s"),
    ("another", "Another", "আরেকটি"),
    ("not_this_one", "Not this one", "এটি নয়"),
    ("on_this_day", "On this day", "এই দিনে"),
    # Note
    ("start_writing", "Start writing…", "লিখতে শুরু করুন…"),
    ("empty_note_tap", "This note is empty. Tap here to write.", "এই নোট ফাঁকা। লিখতে এখানে চাপুন।"),
    ("note_info", "About this note", "এই নোট সম্পর্কে"),
    ("ask_ai_about_note", "Ask AI about this note", "এই নোট নিয়ে AI-কে জিজ্ঞেস করুন"),
    ("version_history", "Version history", "সংস্করণের ইতিহাস"),
    ("local_graph", "Linked notes graph", "যুক্ত নোটের গ্রাফ"),
    ("lock_with_password", "Lock with a password", "পাসওয়ার্ড দিয়ে লক করুন"),
    ("remove_lock", "Remove the lock", "লক সরান"),
    ("merged_with_changes", "This note changed elsewhere too. Both versions were kept; look for the marked part.", "নোটটি অন্য জায়গায়ও বদলেছে। দুটি সংস্করণই রাখা হয়েছে; চিহ্নিত অংশটি দেখুন।"),
    ("words_and_changed", "%1$d words · changed %2$s", "%1$d শব্দ · বদলানো হয়েছে %2$s"),
    ("outline", "Outline", "রূপরেখা"),
    ("linked_from_n", "Linked from (%1$d)", "যেখান থেকে লিংক (%1$d)"),
    ("no_backlinks", "No other note links here yet.", "এখনো অন্য কোনো নোট এখানে লিংক করেনি।"),
    ("mentioned_in", "Mentioned without a link", "লিংক ছাড়া উল্লেখ"),
    ("link_it", "Link it", "লিংক করুন"),
    ("links_to", "Links to", "যেখানে লিংক করে"),
    ("heading", "Heading", "শিরোনাম"),
    ("bold", "Bold", "গাঢ়"),
    ("italic", "Italic", "বাঁকা"),
    ("task", "Task", "কাজ"),
    ("link_to_note", "Link to a note", "নোটে লিংক"),
    ("tag", "Tag", "ট্যাগ"),
    ("quote", "Quote", "উদ্ধৃতি"),
    ("code", "Code", "কোড"),
    ("or_tell_ai", "Or tell the AI what to do…", "অথবা AI-কে বলুন কী করতে হবে…"),
    ("replace_note", "Replace the note", "নোটটি বদলে দিন"),
    ("add_to_note", "Add to the note", "নোটে যোগ করুন"),
    ("thinking", "Thinking…", "ভাবছে…"),
    ("delete_q", "Delete?", "মুছবেন?"),
    # Locks
    ("unlock", "Unlock", "আনলক করুন"),
    ("unlock_title", "Unlock “%1$s”", "“%1$s” আনলক করুন"),
    ("password", "Password", "পাসওয়ার্ড"),
    ("password_again", "Password again", "আবার পাসওয়ার্ড"),
    ("hint_is", "Hint: %1$s", "ইঙ্গিত: %1$s"),
    ("hint_optional", "Hint (optional)", "ইঙ্গিত (ঐচ্ছিক)"),
    ("at_least_6", "At least 6 characters", "অন্তত ৬ অক্ষর"),
    ("wrong_password", "That password isn't right.", "পাসওয়ার্ডটি ঠিক নয়।"),
    ("lock", "Lock", "লক করুন"),
    ("lock_note", "Lock this note", "এই নোট লক করুন"),
    ("lock_folder", "Lock this folder", "এই ফোল্ডার লক করুন"),
    ("lock_explain", "The note is encrypted on disk and in sync. Without the password it can't be read, not even by you, so keep it somewhere safe.", "নোটটি ডিস্কে ও সিঙ্কে এনক্রিপ্ট করা থাকবে। পাসওয়ার্ড ছাড়া এটি পড়া যাবে না, আপনিও পারবেন না, তাই পাসওয়ার্ডটি নিরাপদে রাখুন।"),
    ("note_locked", "Locked. Marrow forgets the password when you leave the app for a while.", "লক হয়েছে। কিছুক্ষণ অ্যাপের বাইরে থাকলে ম্যারো পাসওয়ার্ড ভুলে যায়।"),
    ("locked_note_text", "This note is locked. Enter its password to read it.", "এই নোটটি লক করা। পড়তে এর পাসওয়ার্ড দিন।"),
    ("locked_folder_text", "This note is in a locked folder. Enter the folder's password to read it.", "এই নোটটি একটি লক করা ফোল্ডারে আছে। পড়তে ফোল্ডারের পাসওয়ার্ড দিন।"),
    ("locked_notes", "Locked notes", "লক করা নোট"),
    ("forget_passwords", "Forget passwords", "পাসওয়ার্ড ভুলে যান"),
    ("lock_when_leaving", "When I leave the app", "অ্যাপ ছেড়ে গেলে"),
    ("lock_after_minutes", "After %1$d minutes away", "%1$d মিনিট বাইরে থাকলে"),
    ("lock_after_hour", "After an hour away", "এক ঘণ্টা বাইরে থাকলে"),
    ("lock_all_now", "Lock all now", "এখনই সব লক করুন"),
    ("lock_all_now_subtitle", "Forget every password entered", "দেওয়া সব পাসওয়ার্ড ভুলে যান"),
    ("all_locked", "Every locked note is locked again.", "লক করা সব নোট আবার লক হয়েছে।"),
    # Search
    ("search_your_notes", "Search your notes", "আপনার নোট খুঁজুন"),
    ("search_hint", "Finds words in every note. Spelling variants in Bangla are found too.", "প্রতিটি নোটে শব্দ খোঁজে। বাংলা বানানের ভিন্ন রূপও খুঁজে পায়।"),
    ("recent_searches", "Recent searches", "সাম্প্রতিক খোঁজ"),
    ("notes_named", "Notes named like this", "এই নামের নোট"),
    ("nothing_found", "Nothing found", "কিছু পাওয়া যায়নি"),
    ("nothing_found_text", "Try fewer or different words.", "কম বা অন্য শব্দ দিয়ে চেষ্টা করুন।"),
    # Tasks
    ("show_done", "Show done", "সম্পন্ন দেখান"),
    ("overdue", "Overdue", "সময় পেরিয়েছে"),
    ("next_7_days", "Next 7 days", "আগামী ৭ দিন"),
    ("later", "Later", "পরে"),
    ("no_date", "No date", "তারিখ নেই"),
    ("done_tasks", "Done", "সম্পন্ন"),
    ("no_tasks", "No tasks", "কোনো কাজ নেই"),
    ("no_tasks_text", "Write - [ ] in any note to add a task. Add a date like 📅 2026-11-02 to see it here when it's due.", "যেকোনো নোটে - [ ] লিখে কাজ যোগ করুন। 📅 2026-11-02 এর মতো তারিখ দিলে সময়মতো এখানে দেখা যাবে।"),
    # Ask
    ("ask_your_notes", "Ask your notes", "নোটকে জিজ্ঞেস করুন"),
    ("ask_placeholder", "Ask your notes anything…", "নোটকে যা খুশি জিজ্ঞেস করুন…"),
    ("ask_intro", "Marrow finds the parts of your notes that match and sends only those to your AI. Each point in the answer links to the note it comes from.", "ম্যারো আপনার নোটের মিলে যাওয়া অংশ খুঁজে শুধু সেগুলোই আপনার AI-কে পাঠায়। উত্তরের প্রতিটি কথা যে নোট থেকে এসেছে সেখানে লিংক করা থাকে।"),
    ("ask_example_1", "What did I decide about the trip?", "ভ্রমণ নিয়ে আমি কী ঠিক করেছিলাম?"),
    ("ask_example_2", "Summarize my notes from this week", "এই সপ্তাহের নোটগুলোর সারাংশ দিন"),
    ("ask_example_3", "What's still open in my projects?", "আমার প্রকল্পগুলোতে কী এখনো বাকি?"),
    ("ask_setup_text", "Connect an AI you choose (OpenAI, Claude, Gemini, or one running on your own computer) and ask questions answered from your own notes.", "আপনার পছন্দের একটি AI যুক্ত করুন (OpenAI, Claude, Gemini, বা আপনার নিজের কম্পিউটারে চলা একটি) আর নিজের নোট থেকে উত্তর পান।"),
    ("set_up_ai", "Set up an AI", "AI সেট আপ করুন"),
    ("from_your_notes", "From your notes", "আপনার নোট থেকে"),
    ("new_conversation", "New conversation", "নতুন কথোপকথন"),
    # Calendar
    ("previous_month", "Previous month", "আগের মাস"),
    ("next_month", "Next month", "পরের মাস"),
    ("back_to_today", "Back to today", "আজকে ফিরুন"),
    ("daily_notes", "Daily notes", "দৈনিক নোট"),
    # Graph and canvas
    ("missing_notes", "Notes not written yet", "এখনো না লেখা নোট"),
    ("two_steps", "Two links away", "দুই লিংক দূরে"),
    ("graph_empty", "Link notes with [[ ]] and they appear here, joined.", "[[ ]] দিয়ে নোট যুক্ত করলে সেগুলো এখানে জোড়া দেখাবে।"),
    ("fit_to_screen", "Show everything", "সব দেখান"),
    ("add_card", "Card", "কার্ড"),
    ("add_note", "Note", "নোট"),
    ("connect", "Connect", "যুক্ত করুন"),
    ("color", "Colour", "রং"),
    ("card", "Card", "কার্ড"),
    ("write_something", "Write something…", "কিছু লিখুন…"),
    ("find_a_note", "Find a note", "একটি নোট খুঁজুন"),
    ("tap_card_to_connect", "Tap another card to connect", "যুক্ত করতে আরেকটি কার্ডে চাপুন"),
    ("canvas_reloaded", "The canvas changed elsewhere, so it was opened again.", "ক্যানভাসটি অন্য জায়গায় বদলেছে, তাই আবার খোলা হয়েছে।"),
    # Files
    ("open_in_other_app", "Open in another app", "অন্য অ্যাপে খুলুন"),
    ("open_in_other_app_text", "Marrow keeps this file with your notes. Open it with an app that knows this kind of file.", "ম্যারো এই ফাইলটি আপনার নোটের সাথে রাখে। এ ধরনের ফাইল চেনে এমন অ্যাপে খুলুন।"),
    ("no_app_opens", "No app on this phone opens this kind of file.", "এই ফোনের কোনো অ্যাপ এ ধরনের ফাইল খোলে না।"),
    # History
    ("history_explain", "Marrow keeps earlier versions of each note as you write, every few minutes, and thins them out over time.", "লেখার সময় ম্যারো প্রতি কয়েক মিনিটে প্রতিটি নোটের আগের সংস্করণ রাখে, আর সময়ের সাথে কমিয়ে আনে।"),
    ("no_versions", "No earlier versions", "আগের কোনো সংস্করণ নেই"),
    ("no_versions_text", "Versions appear here once the note has been changed a few times.", "নোটটি কয়েকবার বদলালে সংস্করণগুলো এখানে দেখা যাবে।"),
    ("restore_this_version", "Restore this version", "এই সংস্করণ ফিরিয়ে আনুন"),
    ("restore", "Restore", "ফিরিয়ে আনুন"),
    ("restore_q", "Restore this version?", "এই সংস্করণ ফিরিয়ে আনবেন?"),
    ("restore_text", "The note as it is now is kept as a version too, so you can switch back.", "নোটের বর্তমান রূপটিও একটি সংস্করণ হিসেবে রাখা হবে, তাই আবার ফিরে যেতে পারবেন।"),
    ("restored", "Restored.", "ফিরিয়ে আনা হয়েছে।"),
    ("your_vault_on", "Your vault on", "আপনার ভল্ট যেদিন"),
    ("tm_summary", "%1$d notes · %2$d changed since · %3$d deleted since", "%1$dটি নোট · %2$dটি পরে বদলেছে · %3$dটি পরে মুছেছে"),
    ("deleted_since", "Deleted since", "পরে মুছে ফেলা"),
    ("changed_since", "Changed since", "পরে বদলানো"),
    ("unchanged", "Unchanged", "অপরিবর্তিত"),
    ("open_as_it_is_now", "Open as it is now", "এখনকার রূপে খুলুন"),
    # Settings
    ("vault", "Vault", "ভল্ট"),
    ("app", "App", "অ্যাপ"),
    ("sync", "Sync", "সিঙ্ক"),
    ("sync_off", "Off: notes stay on this phone", "বন্ধ: নোট এই ফোনেই থাকে"),
    ("encrypted", "encrypted", "এনক্রিপ্টেড"),
    ("import_notes", "Import notes", "নোট আমদানি"),
    ("import_subtitle", "From Notion, Evernote, Word and more", "Notion, Evernote, Word ইত্যাদি থেকে"),
    ("conflicts", "Conflicting copies", "সাংঘর্ষিক কপি"),
    ("conflicts_subtitle", "Merge copies sync apps made", "সিঙ্ক অ্যাপের বানানো কপি একত্র করুন"),
    ("appearance", "Appearance", "চেহারা"),
    ("appearance_subtitle", "Theme, note text, text size", "থিম, নোটের লেখা, লেখার আকার"),
    ("language", "Language", "ভাষা"),
    ("same_as_phone", "Same as the phone", "ফোনের মতো"),
    ("ai", "AI", "AI"),
    ("ai_off", "Off", "বন্ধ"),
    ("about", "About", "সম্পর্কে"),
    ("about_text", "Plain Markdown notes that stay yours. Free and open source (MIT).", "সাধারণ মার্কডাউন নোট, যা আপনারই থাকে। বিনামূল্যে ও ওপেন সোর্স (MIT)।"),
    ("website", "Website and source code", "ওয়েবসাইট ও সোর্স কোড"),
    ("theme", "Theme", "থিম"),
    ("light", "Light", "হালকা"),
    ("dark", "Dark", "গাঢ়"),
    ("note_text", "Note text", "নোটের লেখা"),
    ("font_book", "Book", "বই"),
    ("font_sans", "Clean", "পরিষ্কার"),
    ("font_classic", "Classic", "ক্লাসিক"),
    ("text_size", "Text size", "লেখার আকার"),
    ("size_small", "Small", "ছোট"),
    ("size_medium", "Medium", "মাঝারি"),
    ("size_large", "Large", "বড়"),
    ("size_larger", "Larger", "আরও বড়"),
    ("open_notes_for", "Open notes for", "নোট খুলুন"),
    ("reading", "Reading", "পড়ার জন্য"),
    ("editing", "Editing", "সম্পাদনার জন্য"),
    ("preview", "Preview", "নমুনা"),
    ("preview_title", "A quiet place to think", "ভাবার জন্য একটি শান্ত জায়গা"),
    ("preview_text", "Notes are plain text files you own. Write freely; Marrow keeps everything safe as you type.", "নোট হলো আপনার নিজের সাধারণ টেক্সট ফাইল। মন খুলে লিখুন; লেখার সাথে সাথে ম্যারো সব নিরাপদে রাখে।"),
    ("vault_explain", "A vault is the folder your notes live in, as plain Markdown files. The same vault works in Marrow on a computer, and in Obsidian.", "ভল্ট হলো যে ফোল্ডারে আপনার নোট সাধারণ মার্কডাউন ফাইল হিসেবে থাকে। একই ভল্ট কম্পিউটারের ম্যারোতে ও Obsidian-এ কাজ করে।"),
    ("daily_folder", "Daily notes folder", "দৈনিক নোটের ফোল্ডার"),
    ("daily_layout", "Daily notes are kept", "দৈনিক নোট রাখা হয়"),
    ("layout_year_month", "In year and month folders", "বছর ও মাসের ফোল্ডারে"),
    ("layout_flat", "All in one folder", "সব এক ফোল্ডারে"),
    ("templates", "Templates", "টেমপ্লেট"),
    ("templates_folder", "Templates folder", "টেমপ্লেটের ফোল্ডার"),
    ("other_vaults", "Other vaults", "অন্য ভল্ট"),
    ("open_another_vault", "Open another vault", "অন্য ভল্ট খুলুন"),
    ("open_another_vault_text", "Start a new one, or use a folder on this phone", "নতুন একটি শুরু করুন, বা এই ফোনের একটি ফোল্ডার ব্যবহার করুন"),
    ("open_another_vault_confirm", "This vault stays as it is on this phone; you can come back to it any time.", "এই ভল্টটি ফোনে যেমন আছে তেমনই থাকবে; যেকোনো সময় ফিরে আসতে পারবেন।"),
    # Sync
    ("sync_explain", "Keep this vault the same on your phone, tablet and computer. Sign in to your cloud and Marrow keeps your notes there, free with the storage your account already has. Turn on encryption and the cloud only ever sees sealed files.", "আপনার ফোন, ট্যাবলেট ও কম্পিউটারে এই ভল্ট একই রাখুন। আপনার ক্লাউডে সাইন ইন করলে ম্যারো নোটগুলো সেখানে রাখে, আপনার অ্যাকাউন্টের স্টোরেজেই, আলাদা খরচ ছাড়া। এনক্রিপশন চালু করলে ক্লাউড শুধু সিল করা ফাইল দেখে।"),
    ("sync_own_server", "Your own server (WebDAV)", "নিজের সার্ভার (WebDAV)"),
    ("sync_own_server_text", "Nextcloud, ownCloud, Synology, pCloud, Koofr and more", "Nextcloud, ownCloud, Synology, pCloud, Koofr ও আরও"),
    ("cloud_sign_in_to", "Sign in to %1$s", "%1$s-এ সাইন ইন করুন"),
    ("cloud_goes_in", "Notes go in %1$s", "নোট থাকবে %1$s-এ"),
    ("cloud_signing_in", "Finish signing in, then come back to Marrow.", "সাইন-ইন শেষ করে ম্যারোতে ফিরে আসুন।"),
    ("cloud_signed_in_as", "Signed in as %1$s", "%1$s হিসেবে সাইন ইন করা আছে"),
    ("cloud_private", "Your notes go in %1$s, as plain Markdown files you can open there too. Marrow can't see anything else in your %2$s.", "আপনার নোট থাকবে %1$s-এ, সাধারণ Markdown ফাইল হিসেবে, যা সেখানেও খোলা যায়। আপনার %2$s-এর আর কিছুই ম্যারো দেখতে পায় না।"),
    ("cloud_vault_folder", "Vault folder", "ভল্টের ফোল্ডার"),
    ("cloud_new_folder", "A new folder for this vault", "এই ভল্টের জন্য নতুন ফোল্ডার"),
    ("cloud_folder_name", "Folder name", "ফোল্ডারের নাম"),
    ("cloud_use_another", "Use another account", "অন্য অ্যাকাউন্ট ব্যবহার করুন"),
    ("cloud_no_google", "Signing in to Google needs Google Play services on this phone.", "Google-এ সাইন ইন করতে এই ফোনে Google Play services লাগবে।"),
    ("cloud_cancelled", "Sign-in was cancelled.", "সাইন-ইন বাতিল হয়েছে।"),
    ("cloud_folder_new_empty", "A new, empty folder: everything in this vault will be copied there.", "নতুন খালি ফোল্ডার: এই ভল্টের সবকিছু সেখানে কপি হবে।"),
    ("icloud", "iCloud", "iCloud"),
    ("ios_icloud_text", "Your iPhone, iPad and Mac, with your iCloud storage", "আপনার iPhone, iPad ও Mac, আপনার iCloud স্টোরেজে"),
    ("ios_icloud_off", "Sign in to iCloud in the Settings app and turn on iCloud Drive, then come back.", "Settings অ্যাপে iCloud-এ সাইন ইন করে iCloud Drive চালু করুন, তারপর ফিরে আসুন।"),
    ("ios_icloud_where", "Your notes go in iCloud Drive › Marrow › %1$s. On a Mac, open that folder in Marrow, or sync with it.", "আপনার নোট থাকবে iCloud Drive › Marrow › %1$s-এ। Mac-এ ম্যারোতে ওই ফোল্ডার খুলুন, অথবা তার সাথে সিঙ্ক করুন।"),
    ("server_address", "Server address", "সার্ভারের ঠিকানা"),
    ("server_address_hint", "The folder to sync with. In Nextcloud: Files → Settings → WebDAV, then add a folder name.", "যে ফোল্ডারের সাথে সিঙ্ক হবে। Nextcloud-এ: Files → Settings → WebDAV, তারপর একটি ফোল্ডারের নাম যোগ করুন।"),
    ("user_name", "User name", "ব্যবহারকারীর নাম"),
    ("app_password_hint", "Best: an app password made for Marrow in your server's security settings.", "সবচেয়ে ভালো: সার্ভারের নিরাপত্তা সেটিংসে ম্যারোর জন্য বানানো একটি অ্যাপ পাসওয়ার্ড।"),
    ("encrypt_end_to_end", "Encrypt end to end", "এন্ড-টু-এন্ড এনক্রিপ্ট করুন"),
    ("encrypt_explain", "Notes are sealed on this phone with a passphrase; the server can't read them.", "নোট এই ফোনেই পাসফ্রেজ দিয়ে সিল করা হয়; সার্ভার সেগুলো পড়তে পারে না।"),
    ("sync_passphrase", "Sync passphrase", "সিঙ্ক পাসফ্রেজ"),
    ("passphrase_again", "Passphrase again", "আবার পাসফ্রেজ"),
    ("passphrase_new", "At least 8 characters. Use the same one on every device. Without it the notes there can't be read.", "অন্তত ৮ অক্ষর। প্রতিটি ডিভাইসে একই পাসফ্রেজ দিন। এটি ছাড়া সেখানকার নোট পড়া যাবে না।"),
    ("passphrase_existing", "This folder is already encrypted: enter the passphrase you chose for it.", "এই ফোল্ডার আগেই এনক্রিপ্ট করা: এর জন্য বেছে নেওয়া পাসফ্রেজটি দিন।"),
    ("place_empty", "Signed in. The folder is empty: this vault will be copied there.", "সাইন ইন হয়েছে। ফোল্ডারটি খালি: এই ভল্ট সেখানে কপি হবে।"),
    ("place_has_files", "Signed in. The folder has %1$d files: they'll be merged with this vault.", "সাইন ইন হয়েছে। ফোল্ডারে %1$dটি ফাইল আছে: এগুলো এই ভল্টের সাথে মিলবে।"),
    ("start_syncing", "Start syncing", "সিঙ্ক শুরু করুন"),
    ("sync_started", "Syncing.", "সিঙ্ক হচ্ছে।"),
    ("end_to_end_encrypted", "End-to-end encrypted", "এন্ড-টু-এন্ড এনক্রিপ্টেড"),
    ("syncing", "Syncing…", "সিঙ্ক হচ্ছে…"),
    ("last_synced", "Last synced %1$s", "শেষ সিঙ্ক %1$s"),
    ("not_synced_yet", "Not synced yet", "এখনো সিঙ্ক হয়নি"),
    ("sync_now", "Sync now", "এখনই সিঙ্ক করুন"),
    ("sync_anyway", "Sync anyway", "তবুও সিঙ্ক করুন"),
    ("sync_how", "Marrow syncs soon after each change, every 15 seconds while it's open, and when you come back to it. Notes changed in two places at once are merged.", "প্রতিটি বদলের পরপরই, খোলা থাকলে প্রতি ১৫ সেকেন্ডে, আর অ্যাপে ফিরে এলে ম্যারো সিঙ্ক করে। দুই জায়গায় একসাথে বদলানো নোট একত্র করা হয়।"),
    ("turn_off_sync", "Turn off sync", "সিঙ্ক বন্ধ করুন"),
    ("turn_off_sync_q", "Turn off sync?", "সিঙ্ক বন্ধ করবেন?"),
    ("turn_off_sync_text", "Nothing is deleted, here or on the server. Changes simply stop moving between them.", "এখানে বা সার্ভারে কিছুই মোছা হবে না। শুধু দুইয়ের মধ্যে বদলগুলো আর আদান-প্রদান হবে না।"),
    ("turn_off", "Turn off", "বন্ধ করুন"),
    # AI settings
    ("ai_explain", "Marrow works with the AI you choose. Your key stays on this phone, in its secure storage. Nothing is sent until you ask something.", "ম্যারো আপনার বেছে নেওয়া AI-এর সাথে কাজ করে। আপনার কী এই ফোনের নিরাপদ স্টোরেজেই থাকে। আপনি কিছু না জিজ্ঞেস করা পর্যন্ত কিছুই পাঠানো হয় না।"),
    ("no_ai_yet", "No AI set up", "কোনো AI সেট আপ করা নেই"),
    ("no_ai_yet_text", "Add one to ask your notes questions, summarize, translate and more.", "নোটকে প্রশ্ন করতে, সারাংশ, অনুবাদ ও আরও অনেক কিছুর জন্য একটি যোগ করুন।"),
    ("your_ais", "Your AIs (the marked one is used)", "আপনার AI (চিহ্নিতটি ব্যবহার হয়)"),
    ("add_an_ai", "Add an AI", "একটি AI যোগ করুন"),
    ("no_model", "No model chosen", "কোনো মডেল বাছা হয়নি"),
    ("no_key", "no key", "কী নেই"),
    ("address", "Address", "ঠিকানা"),
    ("api_key", "API key", "API কী"),
    ("key_saved", "Saved (leave empty to keep it)", "সংরক্ষিত (রাখতে চাইলে খালি রাখুন)"),
    ("get_a_key", "Get a key", "একটি কী নিন"),
    ("model", "Model", "মডেল"),
    ("ai_says", "It answered: %1$s", "উত্তর দিয়েছে: %1$s"),
    # Import
    ("import_explain", "Bring notes in from another app. They go into a new folder, as plain Markdown, with their pictures and attachments.", "অন্য অ্যাপ থেকে নোট নিয়ে আসুন। ছবি ও সংযুক্তিসহ সাধারণ মার্কডাউন হিসেবে নতুন একটি ফোল্ডারে যাবে।"),
    ("import_notion", "Notion", "Notion"),
    ("import_notion_how", "In Notion: Settings → Export all workspace content, as Markdown & CSV. Pick the .zip it gives you.", "Notion-এ: Settings → Export all workspace content, Markdown & CSV হিসেবে। যে .zip পাবেন সেটি বেছে নিন।"),
    ("import_evernote", "Evernote", "Evernote"),
    ("import_evernote_how", "In Evernote: export notebooks as .enex files, then pick them here.", "Evernote-এ: নোটবুক .enex ফাইল হিসেবে এক্সপোর্ট করুন, তারপর এখানে বেছে নিন।"),
    ("import_word", "Word and OneNote", "Word ও OneNote"),
    ("import_word_how", "Pick .docx files (OneNote can save pages as Word documents).", ".docx ফাইল বেছে নিন (OneNote পাতাগুলো Word ফাইল হিসেবে সংরক্ষণ করতে পারে)।"),
    ("import_markdown", "Markdown, text and web pages", "মার্কডাউন, টেক্সট ও ওয়েব পেজ"),
    ("import_markdown_how", "Pick .md, .txt or .html files.", ".md, .txt বা .html ফাইল বেছে নিন।"),
    ("choose_files", "Choose files", "ফাইল বেছে নিন"),
    ("importing", "Importing…", "আমদানি হচ্ছে…"),
    ("open_folder", "Open folder", "ফোল্ডার খুলুন"),
    # Conflicts
    ("conflicts_explain", "When a note changes on two devices before they sync, a sync app may keep both as a copy. Merge puts the copy's changes into the note and moves the copy to the trash.", "সিঙ্কের আগেই দুই ডিভাইসে নোট বদলালে সিঙ্ক অ্যাপ দুটোকেই কপি হিসেবে রাখতে পারে। একত্র করলে কপির বদলগুলো নোটে যায় আর কপিটি ট্র্যাশে যায়।"),
    ("no_conflicts", "No conflicting copies", "কোনো সাংঘর্ষিক কপি নেই"),
    ("no_conflicts_text", "Everything is in step.", "সব ঠিকঠাক মিলে আছে।"),
    ("copy_named", "Copy: %1$s", "কপি: %1$s"),
    ("merge", "Merge", "একত্র করুন"),
    ("see_copy", "See the copy", "কপিটি দেখুন"),
    ("merged", "Merged.", "একত্র হয়েছে।"),
    ("merged_both_kept", "Merged. Where both changed the same lines, both were kept.", "একত্র হয়েছে। যেখানে দুটোই একই লাইন বদলেছে, সেখানে দুটোই রাখা হয়েছে।"),
    # Welcome
    ("welcome_tagline", "Notes that stay yours: plain Markdown files, linked like your thoughts.", "যে নোট আপনারই থাকে: সাধারণ মার্কডাউন ফাইল, আপনার ভাবনার মতোই একে অপরের সাথে যুক্ত।"),
    ("start_writing_title", "Start writing", "লেখা শুরু করুন"),
    ("start_writing_text", "A new vault on this phone, with a short welcome", "এই ফোনে একটি নতুন ভল্ট, ছোট একটি স্বাগত বার্তাসহ"),
    ("sync_from_server_title", "Get my notes from a server", "সার্ভার থেকে আমার নোট আনুন"),
    ("sync_from_server_text", "Sync with Nextcloud or another WebDAV server", "Nextcloud বা অন্য WebDAV সার্ভারের সাথে সিঙ্ক করুন"),
    ("use_phone_folder_title", "Use a folder on this phone", "এই ফোনের একটি ফোল্ডার ব্যবহার করুন"),
    ("use_phone_folder_text", "For a vault kept in sync by Syncthing or another app", "Syncthing বা অন্য অ্যাপে সিঙ্ক থাকা ভল্টের জন্য"),
    ("folder_not_on_phone", "Pick a folder in the phone's own storage.", "ফোনের নিজস্ব স্টোরেজের একটি ফোল্ডার বেছে নিন।"),
    ("welcome_private", "No account. No tracking. Your notes never leave your devices unless you set up sync.", "কোনো অ্যাকাউন্ট নেই। কোনো নজরদারি নেই। সিঙ্ক চালু না করলে আপনার নোট কখনো আপনার ডিভাইসের বাইরে যায় না।"),
]

# Used on iOS (also kept for Android, where they're unused).
STRINGS += [
    ("ios_a_year_ago", "A year ago", "এক বছর আগে"),
    ("ios_linked_from_one", "Linked from 1 note", "১টি নোট থেকে লিংক"),
    ("ios_one_note", "1 note", "১টি নোট"),
    ("ios_one_imported", "1 note imported into “%1$s”", "১টি নোট “%1$s”-এ আমদানি হয়েছে"),
    ("ios_none", "None", "কোনোটি নয়"),
    ("ios_sync_files", "Sync with a folder in Files (iCloud Drive…)", "Files-এর একটি ফোল্ডারের সাথে সিঙ্ক করুন (iCloud Drive…)"),
    ("ios_sync_files_footer", "Marrow on a Mac can use the same folder.", "Mac-এর ম্যারোও একই ফোল্ডার ব্যবহার করতে পারে।"),
    ("ios_open_folder", "Open a folder", "একটি ফোল্ডার খুলুন"),
    ("ios_open_folder_text", "From Files: iCloud Drive, or a folder on this iPhone", "Files থেকে: iCloud Drive, বা এই iPhone-এর একটি ফোল্ডার"),
    ("ios_note_locked", "This note is locked.", "এই নোটটি লক করা।"),
    ("ios_folder_locked", "The folder “%1$s” is locked.", "“%1$s” ফোল্ডারটি লক করা।"),
    ("ios_changed_elsewhere", "This note was changed somewhere else.", "নোটটি অন্য জায়গায় বদলানো হয়েছে।"),
]

# key: (English one, English other, Bangla) — Bangla has one form.
PLURALS = {
    "linked_from": ("Linked from 1 note", "Linked from %1$d notes", "%1$dটি নোট থেকে লিংক"),
    "links_updated": ("1 link was updated", "%1$d links were updated", "%1$dটি লিংক হালনাগাদ হয়েছে"),
    "notes_found": ("1 note", "%1$d notes", "%1$dটি নোট"),
    "notes_locked": ("Locked 1 note", "Locked %1$d notes", "%1$dটি নোট লক হয়েছে"),
    "years_ago": ("A year ago", "%1$d years ago", "%1$d বছর আগে"),
    "imported_notes": ("1 note imported into “%2$s”", "%1$d notes imported into “%2$s”", "%1$dটি নোট “%2$s”-এ আমদানি হয়েছে"),
}


def android_escape(s: str) -> str:
    s = escape(s).replace("'", "\\'").replace('"', '\\"')
    if s.startswith("@") or s.startswith("?"):
        s = "\\" + s
    return s


def write_android():
    res = os.path.join(APPS, "android", "app", "src", "main", "res")
    for lang, idx in (("", 1), ("-bn", 2)):
        out = ['<?xml version="1.0" encoding="utf-8"?>', "<!-- Written by apps/i18n/strings.py: change the words there. -->", "<resources>"]
        for key, en, bn in STRINGS:
            text = en if idx == 1 else bn
            fmt = ' formatted="false"' if "%" in text and not re.search(r"%\d\$", text) else ""
            out.append(f'    <string name="{key}"{fmt}>{android_escape(text)}</string>')
        for key, (one, other, bn) in PLURALS.items():
            out.append(f'    <plurals name="{key}">')
            if idx == 1:
                out.append(f'        <item quantity="one">{android_escape(one)}</item>')
                out.append(f'        <item quantity="other">{android_escape(other)}</item>')
            else:
                out.append(f'        <item quantity="one">{android_escape(bn)}</item>')
                out.append(f'        <item quantity="other">{android_escape(bn)}</item>')
            out.append("    </plurals>")
        out.append("</resources>\n")
        d = os.path.join(res, "values" + lang)
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "strings.xml"), "w", encoding="utf-8") as f:
            f.write("\n".join(out))


def ios_key(s: str) -> str:
    """Android format → the key SwiftUI looks up: %1$s → %@, %1$d → %lld."""
    return re.sub(r"%\d\$d", "%lld", re.sub(r"%\d\$s", "%@", s))


def ios_value(s: str) -> str:
    """Positions kept, so a translation can reorder: %2$s → %2$@."""
    return re.sub(r"%(\d)\$d", r"%\1$lld", re.sub(r"%(\d)\$s", r"%\1$@", s))


def write_ios():
    strings = {}
    for key, en, bn in STRINGS:
        k = ios_key(en)
        if k in strings:
            continue
        strings[k] = {
            "extractionState": "manual",
            "localizations": {
                "en": {"stringUnit": {"state": "translated", "value": ios_value(en)}},
                "bn": {"stringUnit": {"state": "translated", "value": ios_value(bn)}},
            },
        }
    for key, (one, other, bn) in PLURALS.items():
        k = ios_key(other)
        strings[k] = {
            "extractionState": "manual",
            "localizations": {
                "en": {"variations": {"plural": {
                    "one": {"stringUnit": {"state": "translated", "value": ios_value(one)}},
                    "other": {"stringUnit": {"state": "translated", "value": ios_value(other)}},
                }}},
                "bn": {"variations": {"plural": {
                    "other": {"stringUnit": {"state": "translated", "value": ios_value(bn)}},
                }}},
            },
        }
    catalog = {"sourceLanguage": "en", "strings": dict(sorted(strings.items())), "version": "1.0"}
    path = os.path.join(APPS, "ios", "Marrow", "Localizable.xcstrings")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(catalog, f, ensure_ascii=False, indent=2)
        f.write("\n")


if __name__ == "__main__":
    keys = [k for k, _, _ in STRINGS]
    dupes = {k for k in keys if keys.count(k) > 1}
    assert not dupes, f"repeated keys: {dupes}"
    for k, en, bn in STRINGS:
        assert re.findall(r"%\d\$[sd]", en) == re.findall(r"%\d\$[sd]", bn) or sorted(re.findall(r"%\d\$[sd]", en)) == sorted(re.findall(r"%\d\$[sd]", bn)), f"{k}: formats differ"
    write_android()
    write_ios()
    print(f"{len(STRINGS)} strings, {len(PLURALS)} plurals")
