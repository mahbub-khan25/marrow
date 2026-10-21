# Contributing to Marrow

Thanks for helping. Marrow aims to stay simple, fast and safe with your notes,
so changes are judged on those three things first.

## Reporting a bug

[Open an issue](https://github.com/mahbub-khan25/marrow/issues/new/choose)
and say what you did, what you expected and what happened. Attach the log
(Settings → About → Show the log); it never contains note text. Please don't
attach private notes.

Security problems (for example, a way to read a locked note without its
password) go through [SECURITY.md](SECURITY.md), not public issues.

## Suggesting a feature

Open an issue first and describe the problem it solves. Small, focused
features that fit the app are most likely to be accepted.

## Making a change

1. Fork the repository and create a branch.
2. Set up as in "Build from source" in the [README](README.md), then
   `npm install` and `npm run tauri dev`.
3. Before opening a pull request, run the same checks as CI:

   ```sh
   npx tsc --noEmit             # type check
   npm test                     # window tests
   cargo test --workspace       # engine and app tests
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. Add a test for any change to saving, renaming, locking or other code that
   touches your files. Data-safety fixes need a test that fails without them.
5. Keep pull requests small and explain the why in the description.

By contributing, you agree that your contribution is licensed under the
[MIT license](LICENSE).

## Be kind

Be respectful and assume good intent. Harassment or personal attacks aren't
tolerated, and the maintainer may remove comments or block people who
behave that way.
