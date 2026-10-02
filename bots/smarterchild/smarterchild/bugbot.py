"""BugBot: a buddy to tell about bugs and ideas. It gathers a report over IM and files
it as a GitHub issue (in tagban/him by default), then sends back the link.

Settings, from the environment (or a .env file):
  HOTLINE_HOST, HOTLINE_PORT   the server, default hotline.vespernet.net:5500
  BUGBOT_LOGIN, BUGBOT_PASSWORD  its account (required)
  BUGBOT_NAME          the name buddies see, default BugBot
  BUGBOT_STATUS        its status line, default: Found a bug? Tell me!
  BUGBOT_DATA          where drafts and reports are kept, default ./data
  GITHUB_TOKEN         a token that can write issues on GITHUB_REPO; without one, reports
                       are only kept in the data folder (reports.json)
  GITHUB_REPO          default tagban/him
  BUGBOT_LABEL         the issues' label, default "from BugBot"
  BUGBOT_SHOTS_REPO    a public repo that holds the screenshots, so the issues can show them
                       (e.g. tagban/bugbot-screenshots); without one they stay in the data folder
  BUGBOT_SHOTS_TOKEN   a token that can write that repo's contents (default GITHUB_TOKEN)
"""

from __future__ import annotations

import asyncio
import base64
import json
import logging
import os
import re
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Awaitable, Callable

from .store import Store

log = logging.getLogger("bugbot")

PER_DAY = 5   # reports one person can file a day
DRAFT_DAYS = 3
SHOTS_PER_REPORT = 3
SHOT_BYTES = 5 * 1024 * 1024

INTRO = ("Hi, I'm BugBot! Tell me about a bug or an idea for HIM (or the Adium and Pidgin plugin), "
         "in as many messages as you like (screenshots welcome: send them as files), then type \"send\". Reports are public on GitHub "
         "(github.com/tagban/him/issues), with your screen name, so leave out anything private.")
HELP = ("Tell me what happened (or your idea), in one message or several. Then:\n"
        "  send - file it\n  show - see what you've written\n  cancel - throw it away\n"
        "  my reports - the ones you've filed")

# Words that already say which app or computer it's about.
APP_WORDS = re.compile(r"\b(iphone|ipad|ios|mac|macos|os x|osx|windows|win\s?1[01]|linux|ubuntu|debian|fedora|"
                       r"flatpak|adium|pidgin|finch|tiger|leopard|g[345]|ppc|powerpc|android)\b", re.I)

Filer = Callable[[str, str], Awaitable[str | None]]   # (title, body) -> the issue's link
Uploader = Callable[[str, bytes], Awaitable[str]]     # (file name, bytes) -> a link that shows it

ASK_SHOT = ("Got a screenshot? Send it to me now (drag it into this window, or Send File), then type \"send\". "
            "Or type \"send\" again to file it without one.")


def picture_kind(data: bytes) -> str | None:
    """The extension for a picture GitHub can show, from its first bytes."""
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return "png"
    if data.startswith(b"\xff\xd8\xff"):
        return "jpg"
    if data[:6] in (b"GIF87a", b"GIF89a"):
        return "gif"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "webp"
    return None


def github_uploader(token: str, repo: str) -> Uploader:
    """Puts screenshots in a repo (Contents API) and links them from raw.githubusercontent.com."""

    def put(path: str, data: bytes) -> dict:
        req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/contents/{path}",
            data=json.dumps({"message": f"BugBot: {path}", "content": base64.b64encode(data).decode()}).encode(),
            method="PUT",
            headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json",
                     "X-GitHub-Api-Version": "2022-11-28", "Content-Type": "application/json",
                     "User-Agent": "BugBot (Hotline IM; +https://github.com/tagban/him)"},
        )
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())

    async def upload(name: str, data: bytes) -> str:
        path = time.strftime("%Y/%m/", time.gmtime()) + name
        reply = await asyncio.to_thread(put, path, data)
        return reply["content"]["download_url"]

    return upload


def github_filer(token: str, repo: str, label: str) -> Filer:
    """Files reports as issues through GitHub's API."""

    def post(payload: dict) -> dict:
        req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/issues",
            data=json.dumps(payload).encode(),
            method="POST",
            headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "BugBot (Hotline IM; +https://github.com/tagban/him)",
                "Content-Type": "application/json",
            },
        )
        with urllib.request.urlopen(req, timeout=20) as r:
            return json.loads(r.read())

    async def file(title: str, body: str) -> str | None:
        payload = {"title": title, "body": body, "labels": [label]}
        try:
            issue = await asyncio.to_thread(post, payload)
        except urllib.error.HTTPError as e:
            if e.code != 422:
                raise
            # The label may not be allowed: file it without one.
            payload.pop("labels")
            issue = await asyncio.to_thread(post, payload)
        return issue.get("html_url")

    return file


class Desk:
    """The conversation: one draft per person, filed on "send"."""

    MAX_FILE = SHOT_BYTES

    def __init__(self, folder: Path, filer: Filer | None, uploader: Uploader | None = None):
        self.drafts = Store(folder / "drafts.json", {})
        self.reports = Store(folder / "reports.json", [])
        self.shots = folder / "shots"
        self.filer, self.uploader = filer, uploader

    def wants_file(self, login: str, name: str, size: int) -> tuple[bool, str | None]:
        """A screenshot for the report: yes, a few, if they're pictures and not huge."""
        draft = self._draft(login)
        if size > SHOT_BYTES:
            return False, "That's too big for me (5 MB at most). A smaller screenshot?"
        if draft and len(draft.get("shots", [])) >= SHOTS_PER_REPORT:
            return False, f"{SHOTS_PER_REPORT} screenshots is plenty for one report! Type \"send\" to file it."
        if not name.lower().endswith((".png", ".jpg", ".jpeg", ".gif", ".webp")):
            return False, "I only take screenshots (PNG, JPEG, GIF or WebP)."
        if not draft and self._filed_today(login) >= PER_DAY:
            return False, "You've filed a lot today, thank you! Give it until tomorrow for more."
        return True, None

    async def got_file(self, login: str, name: str, data: bytes) -> list[str]:
        kind = picture_kind(data)
        if not kind:
            return ["That doesn't look like a picture. A PNG or JPEG screenshot, please?"]
        draft = self._draft(login)
        first = draft is None
        if first:
            draft = {"lines": [], "created": time.time()}
            self.drafts.data[login.lower()] = draft
        self.shots.mkdir(parents=True, exist_ok=True)
        saved = f"{time.strftime('%Y%m%d-%H%M%S')}-{base64.b32encode(os.urandom(5)).decode().lower()}.{kind}"
        (self.shots / saved).write_bytes(data)
        draft.setdefault("shots", []).append(saved)
        draft["updated"] = time.time()
        self.drafts.save()
        log.info("screenshot from %s: %s (%d bytes)", login, saved, len(data))
        if first or not draft["lines"]:
            return ["Got the screenshot! Now tell me what went wrong, then type \"send\"."]
        return ["Got the screenshot. Type \"send\" when you're done."]

    async def _pictures(self, shots: list[str]) -> str:
        """The screenshots, as Markdown the issue shows (or a note that BugBot kept them)."""
        out = []
        for name in shots:
            path = self.shots / name
            url = None
            if self.uploader and path.exists():
                try:
                    url = await self.uploader(name, path.read_bytes())
                except Exception:
                    log.exception("uploading %s", name)
            out.append(f"![screenshot]({url})" if url else f"_Screenshot kept by BugBot: {name}_")
        return "\n\n" + "\n\n".join(out) if out else ""

    def _drop_shots(self, draft: dict) -> None:
        for name in draft.get("shots", []):
            (self.shots / name).unlink(missing_ok=True)

    def _draft(self, login: str) -> dict | None:
        d = self.drafts.data.get(login.lower())
        if d and time.time() - d.get("updated", 0) > DRAFT_DAYS * 86400:
            self._drop_shots(d)
            self.drafts.data.pop(login.lower(), None)
            self.drafts.save()
            return None
        return d

    def _filed_today(self, login: str) -> int:
        since = time.time() - 86400
        return sum(1 for r in self.reports.data if r["login"] == login.lower() and r["at"] > since)

    async def answer(self, login: str, text: str) -> list[str]:
        t = text.strip()
        word = t.lower().rstrip(".!")
        draft = self._draft(login)

        if draft and draft.get("asking_app"):
            draft["app"] = None if word in ("skip", "no", "dunno", "not sure") else t
            draft["asking_app"] = False
            return await self._file(login, draft)

        if word in ("help", "?", "commands"):
            return [HELP]
        if word in ("hi", "hello", "hey", "yo", "sup", "hiya") and not draft:
            return [INTRO]
        if word in ("cancel", "never mind", "nevermind", "forget it", "delete"):
            if not draft:
                return ["There's nothing to cancel."]
            self._drop_shots(draft)
            self.drafts.data.pop(login.lower(), None)
            self.drafts.save()
            return ["Okay, thrown away."]
        if word in ("show", "status", "draft"):
            if not draft:
                return ["You haven't started a report. Just tell me what happened."]
            shots = len(draft.get("shots", []))
            pics = f"\n(and {shots} screenshot{'s' if shots > 1 else ''})" if shots else ""
            return ["So far:\n" + "\n".join(draft["lines"]) + pics + "\n\nType \"send\" to file it."]
        if word in ("my reports", "my bugs", "mine", "reports"):
            mine = [r for r in self.reports.data if r["login"] == login.lower()][-5:]
            if not mine:
                return ["You haven't filed any reports yet."]
            return ["Your reports:\n" + "\n".join(f"- {r['title']}: {r.get('url') or 'saved'}" for r in mine)]
        if word in ("send", "submit", "file it", "done", "that's it", "thats it"):
            if not draft or not draft["lines"]:
                return ["Nothing to send yet: tell me what happened first."]
            if not draft.get("shots") and not draft.get("asked_shot"):
                draft["asked_shot"] = True
                self.drafts.save()
                return [ASK_SHOT]
            if not draft.get("app") and not draft.get("asked_app") and not APP_WORDS.search(" ".join(draft["lines"])):
                draft["asked_app"] = draft["asking_app"] = True
                self.drafts.save()
                return ["Which app is this about, on what? (Like \"HIM 0.1.1 on Windows\" or \"HIM on iPhone\".) "
                        "Or type \"skip\"."]
            return await self._file(login, draft)

        # Anything else is part of the report.
        first = draft is None
        if first:
            if self._filed_today(login) >= PER_DAY:
                return ["You've filed a lot today, thank you! Give it until tomorrow for more."]
            draft = {"lines": [], "created": time.time()}
            self.drafts.data[login.lower()] = draft
        draft["lines"].append(t[:2000])
        draft["updated"] = time.time()
        if len(draft["lines"]) > 40:
            draft["lines"] = draft["lines"][-40:]
        self.drafts.save()
        if first:
            return ["Got it. Add anything else (what you did, what you expected, which app; screenshots as files), then type "
                    "\"send\" to file it. \"cancel\" throws it away. (Reports are public on GitHub, with your "
                    "screen name.)"]
        return ["Added. Type \"send\" when you're done."]

    async def _file(self, login: str, draft: dict) -> list[str]:
        lines = draft["lines"]
        title = re.sub(r"\s+", " ", lines[0]).strip()
        if len(title) > 72:
            title = title[:70].rsplit(" ", 1)[0] + "..."
        when = time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime())
        pictures = await self._pictures(draft.get("shots", []))
        body = "\n\n".join(lines) + pictures + (
            f"\n\n---\n**App:** {draft.get('app') or 'not given'}\n"
            f"Reported over Hotline by **{login}** through BugBot, {when}."
        )
        url = None
        if self.filer:
            try:
                url = await self.filer(title, body)
            except Exception:
                log.exception("filing %s's report", login)
                # Kept below: it isn't lost, and it can be filed by hand.
        self.reports.data.append({"login": login.lower(), "title": title, "body": body, "url": url,
                                  "shots": draft.get("shots", []), "at": time.time()})
        self.reports.save()
        self.drafts.data.pop(login.lower(), None)
        self.drafts.save()
        log.info("report from %s: %s -> %s", login, title, url or "saved")
        if url:
            return [f"Thanks! It's filed: {url}"]
        return ["Thanks! Your report is saved, and it'll be looked at soon."]


def main() -> None:
    from .bot import Bot, load_env

    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"),
                        format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    load_env(Path(".env"))
    data = Path(os.environ.get("BUGBOT_DATA", "data"))
    login, password = os.environ.get("BUGBOT_LOGIN"), os.environ.get("BUGBOT_PASSWORD")
    if not login or not password:
        raise SystemExit("Set BUGBOT_LOGIN and BUGBOT_PASSWORD (in the environment or a .env file).")
    token = os.environ.get("GITHUB_TOKEN", "").strip()
    filer = github_filer(token, os.environ.get("GITHUB_REPO", "tagban/him"),
                         os.environ.get("BUGBOT_LABEL", "from BugBot")) if token else None
    if not filer:
        log.warning("no GITHUB_TOKEN: reports are only kept in %s", data / "reports.json")
    shots_repo = os.environ.get("BUGBOT_SHOTS_REPO", "").strip()
    shots_token = os.environ.get("BUGBOT_SHOTS_TOKEN", "").strip() or token
    uploader = github_uploader(shots_token, shots_repo) if shots_repo and shots_token else None
    if not uploader:
        log.warning("no BUGBOT_SHOTS_REPO: screenshots stay in %s", data / "shots")
    name = os.environ.get("BUGBOT_NAME", "BugBot")
    bot = Bot(os.environ.get("HOTLINE_HOST", "hotline.vespernet.net"), int(os.environ.get("HOTLINE_PORT", "5500")),
              login, password, name, os.environ.get("BUGBOT_STATUS", "Found a bug? Tell me!"), data,
              brain=Desk(data, filer, uploader), app_string="BugBot 0.1", welcome=INTRO, bang_commands=False)
    try:
        asyncio.run(bot.run())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
