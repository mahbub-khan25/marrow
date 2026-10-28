# Privacy

Marrow is a notes app that keeps your notes on your own devices. It has no
account, no servers of its own, no analytics and no tracking.

## Your notes

Your notes are Markdown files in a folder you choose, on your device. Marrow
reads and writes them there and nowhere else, unless you turn on sync or ask
an AI something.

## Sync

When you turn on sync, Marrow copies your notes between your device and the
place you chose:

* **Google Drive:** a folder called `Marrow` in your My Drive. Marrow asks
  Google only for access to the files it creates itself (the `drive.file`
  permission), so it can't see, read or change anything else in your Drive.
* **OneDrive** and **Dropbox:** Marrow's own app folder, `Apps/Marrow`. Marrow
  can't see anything else in your OneDrive or Dropbox.
* **iCloud:** Marrow's folder in your iCloud Drive.
* **Your own server (WebDAV)**, or **a folder** on your device that another
  app syncs.

Your notes go straight from your device to that service; they never pass
through anyone else's computers. When you sign in, the service gives Marrow
a key to that folder, which Marrow keeps in your device's secure storage
(Keychain on Apple devices, the Android Keystore, the system's credential
store on computers). Turning sync off deletes the key; you can also remove
Marrow's access in your Google, Microsoft or Dropbox account settings at any
time.

With **end-to-end encryption** turned on, every note and every file name is
encrypted on your device with your sync passphrase before it leaves, so the
service only stores scrambled data. The passphrase never leaves your devices.

What each service does with the files you store there is covered by its own
privacy policy.

## AI

If you connect an AI (OpenAI, Anthropic, Google, or one on your own
computer) and ask it something, Marrow sends your question and the parts of
your notes that best match it to that AI, using your own key. Nothing is
sent until you ask.

## Questions

Open an issue at https://github.com/mahbub-khan25/marrow/issues.
