# Security policy

I.R.I.S. runs AI agents that can read the web, run commands and use your accounts, so security reports are very welcome.

## Reporting a vulnerability

Please **don't open a public issue**. Use GitHub's private reporting instead: the **Security** tab of this repository → **Report a vulnerability**. Include what you found, how to reproduce it, and what an attacker could do with it.

You'll get an answer as soon as possible. Once a fix is released, you're welcome to be credited.

## Scope

In scope:
- the engine (`crates/`) and its API, including the approval and auto-mode rules and the path, URL and command checks;
- the desktop and web app (`app/`);
- the relay (`relay/`) and the end-to-end encrypted link;
- the Android app (`android/`).

Prompt injection that makes an agent take a risky step **without** your approval (or around the auto-mode rules) is in scope. An agent doing something unwise **after** you approved it is not.

## Known limits

These are design choices, not secrets. Please read them before trusting I.R.I.S. with anything sensitive.

- **Direct connections on your network are not encrypted.** The engine listens on all network interfaces (port 7700) and speaks plain HTTP. The device token travels in the request header, so anyone who can watch that network traffic can take it and get full access. Use direct connections only on a network you trust; use the relay (I.R.I.S. Link, end-to-end encrypted) from anywhere else.
- **Every device token has full access.** There are no read-only devices. Remove a device in Settings if you lose it.
- **Full auto is not a hard boundary.** In *Full* auto, commands run without a second model reviewing them. A fixed list of rules still sends the obviously dangerous ones to you, but a list of rules cannot catch everything a shell can do.
- **The browser version of the app comes from your relay.** A relay that was tampered with could serve modified code and read what you type in the browser. The Android app pins the keys instead.
- **Updates trust GitHub.** The app updates itself from this repository's releases, and checks each file against the SHA-256 checksum GitHub lists for it. That protects against a damaged or swapped download, but not against someone who controls the repository itself: the installer is not code-signed, so its safety rests on your GitHub account. The phone app also checks that an update is signed with the same key as the installed app (Android enforces this too).
- **Saved logins and API keys live in files on your PC.** Logins are encrypted with Windows DPAPI at machine scope and keys are plain files in your data folder, so they are only as private as that folder's permissions.
