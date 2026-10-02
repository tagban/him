import asyncio

from smarterchild import hotline
from smarterchild.hotline import Client, Transaction, Tx, F


def test_im_send_matches_the_guides_worked_example():
    guid = bytes([0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF] * 2)
    t = Transaction(Tx.IM_SEND, [(F.FRIEND_LOGIN, b"alice"), (F.MESSAGE_GUID, guid), (F.MESSAGE_BODY, b"hi")], id=7)
    b = t.encode()
    assert b[:20] == bytes([0, 0, 0x03, 0x2A, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0x25, 0, 0, 0, 0x25])
    assert len(b) == 57
    assert Transaction.decode(b[:20], b[20:]) == t


def test_hmac_sha256_known_answer():
    # RFC 4231 test case 2
    assert hotline.mac("HMAC-SHA256", b"Jefe", b"what do ya want for nothing?").hex() == \
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"


def test_sealer_round_trip():
    enc, dec = hotline.aead_keys("HMAC-SHA256", b"pw", bytes(range(64)))
    assert enc != dec
    a, b = hotline.Sealer(dec, 1), hotline.Sealer(dec, 1)
    for i in range(3):
        assert b.open(a.seal(b"hello %d" % i)) == b"hello %d" % i


async def _pair(port):
    got: dict[str, asyncio.Queue] = {"alice": asyncio.Queue(), "bob": asyncio.Queue()}

    def handler(who):
        async def h(kind, data):
            await got[who].put((kind, data))
        return h

    alice = Client("127.0.0.1", port, "alice", "hotline", on_event=handler("alice"))
    bob = Client("127.0.0.1", port, "bob", "hotline", on_event=handler("bob"))
    await alice.connect()
    await bob.connect()
    return alice, bob, got


async def _next(q, kind, timeout=5):
    while True:
        k, d = await asyncio.wait_for(q.get(), timeout)
        if k == kind:
            return d


def test_sign_on_and_message_over_hope_aead(mock_server):
    async def run():
        alice, bob, got = await _pair(mock_server)
        assert alice.transport == "HOPE (ChaCha20-Poly1305)"
        roster = await bob.get_roster()
        assert any(b.login == "alice" and b.state == hotline.ACCEPTED for b in roster)
        await bob.set_presence(hotline.ONLINE, "Ask me anything!")
        assert await bob.send_im("alice", "héllo ✨") == 0
        m = (await _next(got["alice"], "message"))["message"]
        assert (m.sender, m.body) == ("bob", "héllo ✨")
        # alice's client acknowledged delivery on its own
        ack = await _next(got["bob"], "ack")
        assert ack["login"] == "alice" and not ack["read"]
        await alice.close()
        await bob.close()

    asyncio.run(run())


def test_friend_request_can_be_accepted(mock_server):
    async def run():
        dave = Client("127.0.0.1", mock_server, "dave", "hotline")
        q: asyncio.Queue = asyncio.Queue()

        async def h(kind, data):
            await q.put((kind, data))

        carol = Client("127.0.0.1", mock_server, "carol", "hotline", on_event=h)
        await dave.connect()
        await carol.connect()
        await carol.get_roster()
        await dave.add_friend("carol", "hi from dave")
        req = await _next(q, "friend_request")
        assert req["login"] == "dave"
        await carol.accept("dave")
        roster = await carol.get_roster()
        assert any(b.login == "dave" and b.state == hotline.ACCEPTED for b in roster)
        await dave.close()
        await carol.close()

    asyncio.run(run())


def test_wrong_password_is_refused(mock_server):
    async def run():
        c = Client("127.0.0.1", mock_server, "alice", "nope")
        try:
            await c.connect()
        except hotline.HotlineError as e:
            return str(e)
        raise AssertionError("signed on with a wrong password")

    assert "Incorrect" in asyncio.run(run())


def test_hub_chat_answers_only_when_addressed(mock_server, tmp_path):
    from smarterchild.brain import Brain
    from smarterchild.hub import Hub, addressed

    assert addressed("SmarterChild, what is 6*7", ["SmarterChild"]) == "what is 6*7"
    assert addressed("what is 6*7, smarterchild?", ["smarterchild"]) == "what is 6*7"
    assert addressed("@smarterchild define ennui", ["smarterchild"]) == "define ennui"
    assert addressed("i think smarterchild is neat", ["smarterchild"]) is None

    async def run():
        hub = Hub(Brain(tmp_path), "127.0.0.1", mock_server, "SmarterChild", 168)
        task = asyncio.create_task(hub.session())
        heard: asyncio.Queue = asyncio.Queue()

        async def h(kind, data):
            if kind == "chat":
                await heard.put(data["text"])

        pat = Client("127.0.0.1", mock_server, "", "", nickname="Pat", classic=True, on_event=h)
        await pat.connect()
        for _ in range(50):
            if hub.client and not hub.client.closed.is_set():
                break
            await asyncio.sleep(0.05)
        users = await pat.get_users()
        assert "SmarterChild" in users.values()
        pat.send_chat("nice weather today")
        pat.send_chat("SmarterChild, what is 6*7")
        lines = []
        while True:
            t = await asyncio.wait_for(heard.get(), 6)
            lines.append(t)
            if "42" in t:
                break
        assert not any("SmarterChild:" in l and "nice weather" in l for l in lines[1:])
        assert "Pat: 6*7 = 42" in lines[-1]
        # someone on Discord, through the bridge
        pat.send_chat("Discord | Sam: !calc 6*6")
        while "36" not in (t := await asyncio.wait_for(heard.get(), 6)):
            pass
        assert "Sam: 6*6 = 36" in t
        # and by the name they asked for
        pat.send_chat("Discord | Pat: !call me Patty")  # Pat, from Discord
        pat.send_chat("!calc 5*5")  # and Pat, here
        while "25" not in (t := await asyncio.wait_for(heard.get(), 6)):
            pass
        assert "Patty: 5*5 = 25" in t
        # a private message is answered privately
        got: asyncio.Queue = asyncio.Queue()

        async def pm(kind, data):
            if kind == "private":
                await got.put(data)
        pat.on_event = pm
        sc = next(uid for uid, n in users.items() if n == "SmarterChild")
        pat.send_private(sc, "where are you from?")
        d = await asyncio.wait_for(got.get(), 6)
        assert d["name"] == "SmarterChild" and "chatterbot" in d["text"].lower()
        await pat.close()
        await hub.client.close()
        task.cancel()

    asyncio.run(run())


def test_a_file_goes_through_the_sealed_relay(mock_server):
    picture = bytes(range(256)) * 700  # bigger than a chunk

    async def run():
        alice, bob, got = await _pair(mock_server)
        guid = await alice.offer_file("bob", "../shot.png", len(picture))
        offer = (await _next(got["bob"], "file_offer"))["offer"]
        assert (offer.sender, offer.name, offer.size, offer.guid) == ("alice", "shot.png", len(picture), guid)
        await bob.accept_file(offer.guid)
        up = (await _next(got["alice"], "file_ready"))["relay_ref"]
        down = (await _next(got["bob"], "file_ready"))["relay_ref"]
        _, (name, data) = await asyncio.gather(alice.send_file(up, "shot.png", picture),
                                               bob.receive_file(down, 1 << 20))
        assert (name, data) == ("shot.png", picture)
        # and one that's too big is refused
        guid = await alice.offer_file("bob", "huge.png", len(picture))
        await bob.accept_file((await _next(got["bob"], "file_offer"))["offer"].guid)
        up = (await _next(got["alice"], "file_ready"))["relay_ref"]
        down = (await _next(got["bob"], "file_ready"))["relay_ref"]
        sent = asyncio.create_task(alice.send_file(up, "huge.png", picture))
        try:
            await bob.receive_file(down, 1000)
            raise AssertionError("took a file over the limit")
        except hotline.FileTooBig:
            pass
        await asyncio.wait([sent], timeout=3)
        await alice.close()
        await bob.close()

    asyncio.run(run())


def test_buddy_icon_is_set_once(mock_server, tmp_path):
    from pathlib import Path

    from smarterchild.bot import Bot, load_icon

    icon = load_icon(None, "smarterchild.png")
    assert icon and icon.startswith(b"\x89PNG") and len(icon) < 16384

    async def run():
        bot = Bot("127.0.0.1", mock_server, "bob", "hotline", "Bob", "hi", tmp_path, icon=icon)
        task = asyncio.create_task(bot.session())
        for _ in range(100):
            if bot.client and bot.client.wire:
                break
            await asyncio.sleep(0.05)
        await asyncio.sleep(0.5)
        assert await bot.client.own_icon_hash() == Client.icon_hash(icon)
        alice = Client("127.0.0.1", mock_server, "alice", "hotline")
        await alice.connect()
        r = await alice.request(Tx.GET_BUDDY_ICON, [(F.FRIEND_LOGIN, b"bob")])
        assert r.get(F.BUDDY_ICON) == icon   # a buddy gets the picture
        await alice.close()
        await bot.client.close()
        task.cancel()

    asyncio.run(run())
