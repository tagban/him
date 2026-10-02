# BugBot

A buddy for reporting bugs and ideas. Add **bugbot** on VesperNet in HIM, tell it
what happened (in as many messages as you like, plus screenshots sent as files),
and type **send**. It files a
GitHub issue in [tagban/him](https://github.com/tagban/him/issues), labeled
"from BugBot", and sends you the link.

| | |
|---|---|
| `send` | file the report (it asks once for a screenshot, and which app it's about if you didn't say) |
| `show` | what you've written so far |
| `cancel` | throw it away |
| `my reports` | the ones you've filed |
| `help` | this list |

Reports are public, with the reporter's screen name; BugBot says so before
anything is filed. Five reports per person per day. Every report is also kept in
`data/reports.json`, so nothing is lost if GitHub is down (or there's no token).

It runs from SmarterChild's code (`../smarterchild`, the `bugbot` command).

## Running it

```sh
cp .env.example .env    # then fill in the password and the GitHub token
docker compose up -d --build
docker compose logs -f
```

The GitHub token: github.com > Settings > Developer settings > Fine-grained
tokens > Generate, with access to **tagban/him** only and the permission
**Issues: Read and write**. It goes in `.env` on the server and nowhere else.

### Screenshots

Up to three per report (PNG, JPEG, GIF or WebP, 5 MB each). GitHub's API can't
attach pictures to an issue, so BugBot puts them in a public repo of their own
and links them into the issue:

1. Create an empty public repo, e.g. **tagban/bugbot-screenshots**.
2. A second fine-grained token with access to **that repo only** and
   **Contents: Read and write** (kept separate so BugBot can't write to HIM's code).
3. In `.env`: `BUGBOT_SHOTS_REPO=tagban/bugbot-screenshots` and the token in
   `BUGBOT_SHOTS_TOKEN`.

Without them, screenshots stay in `data/shots/` and the issue names the file.
Screenshots are public like the reports; BugBot says so before filing.
