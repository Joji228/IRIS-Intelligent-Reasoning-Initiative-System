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
