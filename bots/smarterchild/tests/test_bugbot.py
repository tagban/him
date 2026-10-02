"""BugBot's report desk, with a pretend GitHub."""

import asyncio

from smarterchild.bugbot import PER_DAY, Desk


def run(coro):
    return asyncio.run(coro)


class FakeGitHub:
    def __init__(self):
        self.issues = []

    async def __call__(self, title, body):
        self.issues.append((title, body))
        return f"https://github.com/tagban/him/issues/{len(self.issues)}"


def test_a_report_is_gathered_asked_about_the_app_and_filed(tmp_path):
    gh = FakeGitHub()
    desk = Desk(tmp_path, gh)
    first = run(desk.answer("Pat", "Sending a picture does nothing"))[0]
    assert "send" in first and "public" in first
    assert run(desk.answer("Pat", "I tap Send a Picture and nothing opens")) == ['Added. Type "send" when you\'re done.']
    assert "screenshot" in run(desk.answer("Pat", "send"))[0]
    ask = run(desk.answer("Pat", "send"))[0]
    assert "Which app" in ask
    done = run(desk.answer("Pat", "HIM 0.1.1 on iPhone"))[0]
    assert done == "Thanks! It's filed: https://github.com/tagban/him/issues/1"
    title, body = gh.issues[0]
    assert title == "Sending a picture does nothing"
    assert "I tap Send a Picture" in body and "**App:** HIM 0.1.1 on iPhone" in body and "**Pat**" in body
    assert "issues/1" in run(desk.answer("pat", "my reports"))[0]


def test_saying_the_app_up_front_skips_the_question(tmp_path):
    gh = FakeGitHub()
    desk = Desk(tmp_path, gh)
    run(desk.answer("sam", "On Windows the buddy list is empty"))
    run(desk.answer("sam", "send"))  # no screenshot
    assert "filed" in run(desk.answer("sam", "send"))[0]


def test_cancel_and_show(tmp_path):
    desk = Desk(tmp_path, FakeGitHub())
    assert "nothing to cancel" in run(desk.answer("al", "cancel"))[0]
    run(desk.answer("al", "the sound is too loud"))
    assert "the sound is too loud" in run(desk.answer("al", "show"))[0]
    assert run(desk.answer("al", "cancel")) == ["Okay, thrown away."]
    assert "Nothing to send" in run(desk.answer("al", "send"))[0]


def test_a_few_reports_a_day(tmp_path):
    desk = Desk(tmp_path, FakeGitHub())
    for i in range(PER_DAY):
        run(desk.answer("max", f"bug number {i} on mac"))
        run(desk.answer("max", "send"))
        run(desk.answer("max", "send"))
    assert "tomorrow" in run(desk.answer("max", "one more on mac"))[0]


def test_without_github_reports_are_kept(tmp_path):
    desk = Desk(tmp_path, None)
    run(desk.answer("jo", "Linux has no icons"))
    run(desk.answer("jo", "send"))
    assert "saved" in run(desk.answer("jo", "send"))[0]
    assert Desk(tmp_path, None).reports.data[0]["title"] == "Linux has no icons"


def test_github_trouble_doesnt_lose_the_report(tmp_path):
    async def broken(title, body):
        raise OSError("GitHub is down")

    desk = Desk(tmp_path, broken)
    run(desk.answer("lee", "Adium crashes on quit"))
    run(desk.answer("lee", "send"))
    assert "saved" in run(desk.answer("lee", "send"))[0]
    assert desk.reports.data[0]["url"] is None


PNG = b"\x89PNG\r\n\x1a\n" + bytes(100)


def test_screenshots_are_uploaded_into_the_issue(tmp_path):
    gh, uploaded = FakeGitHub(), []

    async def uploader(name, data):
        uploaded.append((name, data))
        return f"https://raw.example/{name}"

    desk = Desk(tmp_path, gh, uploader)
    assert desk.wants_file("kim", "shot.png", len(PNG)) == (True, None)
    assert not desk.wants_file("kim", "notes.txt", 10)[0]
    assert not desk.wants_file("kim", "huge.png", 50 * 1024 * 1024)[0]
    assert "tell me what went wrong" in run(desk.got_file("kim", "shot.png", PNG))[0]
    run(desk.answer("kim", "The Mac buddy list is blank"))
    assert "doesn't look like a picture" in run(desk.got_file("kim", "fake.png", b"hello"))[0]
    assert "1 screenshot" in run(desk.answer("kim", "show"))[0]
    assert "filed" in run(desk.answer("kim", "send"))[0]   # it has a screenshot: no asking
    title, body = gh.issues[0]
    assert title == "The Mac buddy list is blank"
    assert f"![screenshot](https://raw.example/{uploaded[0][0]})" in body and uploaded[0][1] == PNG


def test_cancel_throws_the_screenshots_away(tmp_path):
    desk = Desk(tmp_path, None)
    run(desk.answer("ann", "pidgin won't connect"))
    run(desk.got_file("ann", "a.png", PNG))
    assert len(list((tmp_path / "shots").iterdir())) == 1
    run(desk.answer("ann", "cancel"))
    assert not list((tmp_path / "shots").iterdir())


def test_a_screenshot_sent_over_hotline_reaches_the_report(mock_server, tmp_path):
    from smarterchild.bot import Bot
    from smarterchild.hotline import Client

    async def go():
        gh = FakeGitHub()
        bot = Bot("127.0.0.1", mock_server, "bob", "hotline", "BugBot", "Found a bug?", tmp_path,
                  brain=Desk(tmp_path, gh), bang_commands=False)
        task = asyncio.create_task(bot.session())
        heard: asyncio.Queue = asyncio.Queue()
        ready: asyncio.Queue = asyncio.Queue()

        async def h(kind, data):
            if kind == "message":
                await heard.put(data["message"].body)
            elif kind == "file_ready":
                await ready.put(data["relay_ref"])

        alice = Client("127.0.0.1", mock_server, "alice", "hotline", on_event=h)
        await alice.connect()
        for _ in range(100):
            if bot.client and bot.client.wire:
                break
            await asyncio.sleep(0.05)
        await asyncio.sleep(0.3)
        await alice.send_im("bob", "Pictures don't open on Linux")
        await asyncio.wait_for(heard.get(), 8)
        await alice.offer_file("bob", "shot.png", len(PNG))
        await alice.send_file(await asyncio.wait_for(ready.get(), 8), "shot.png", PNG)
        assert "Got the screenshot" in await asyncio.wait_for(heard.get(), 8)
        await alice.offer_file("bob", "notes.txt", 5)   # not a picture: turned down
        assert "only take screenshots" in await asyncio.wait_for(heard.get(), 8)
        await alice.send_im("bob", "send")
        assert "filed" in await asyncio.wait_for(heard.get(), 8)
        assert "_Screenshot kept by BugBot" in gh.issues[0][1]
        await alice.close()
        await bot.client.close()
        task.cancel()

    asyncio.run(go())
