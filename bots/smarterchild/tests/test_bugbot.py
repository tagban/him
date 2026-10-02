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
    assert "tomorrow" in run(desk.answer("max", "one more on mac"))[0]


def test_without_github_reports_are_kept(tmp_path):
    desk = Desk(tmp_path, None)
    run(desk.answer("jo", "Linux has no icons"))
    assert "saved" in run(desk.answer("jo", "send"))[0]
    assert Desk(tmp_path, None).reports.data[0]["title"] == "Linux has no icons"


def test_github_trouble_doesnt_lose_the_report(tmp_path):
    async def broken(title, body):
        raise OSError("GitHub is down")

    desk = Desk(tmp_path, broken)
    run(desk.answer("lee", "Adium crashes on quit"))
    assert "saved" in run(desk.answer("lee", "send"))[0]
    assert desk.reports.data[0]["url"] is None
