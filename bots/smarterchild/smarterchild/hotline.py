"""A Hotline Instant Messaging client for bots, in Python.

It speaks the same protocol as HIM (see hotline-im/ in this repo, and fogWraith's
Client-Creation-Guide.md): the TRTP handshake, HOPE secure sign-on with
HMAC-SHA256, the HOPE ChaCha20-Poly1305 transport, and the 800-block messaging
transactions. Every integer on the wire is big-endian.
"""

from __future__ import annotations

import asyncio
import hashlib
import hmac
import logging
import os
import struct
from dataclasses import dataclass, field as dfield
from typing import Awaitable, Callable

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

log = logging.getLogger("hotline")

# ---------- identifiers (guide, Appendix B) ----------

class Tx:
    LOGIN = 107
    SHOW_AGREEMENT = 109
    AGREED = 121
    DISCONNECT_MSG = 111
    SERVER_MSG = 104
    SEND_CHAT = 105
    CHAT_MSG = 106
    SEND_INSTANT_MSG = 108
    GET_USER_NAME_LIST = 300
    NOTIFY_CHANGE_USER = 301
    NOTIFY_DELETE_USER = 302
    SET_CLIENT_USER_INFO = 304
    GET_ROSTER = 800
    ROSTER_ENTRY = 801
    ADD_FRIEND = 802
    REMOVE_FRIEND = 803
    FRIEND_REQUEST = 804
    FRIEND_RESPONSE = 805
    SET_PRESENCE = 808
    PRESENCE_CHANGED = 809
    IM_SEND = 810
    IM_DELIVER = 811
    IM_ACK = 812
    IM_TYPING = 813
    GET_USER_INFO = 825


class F:
    ERROR = 100
    DATA = 101
    USER_NAME = 102
    USER_ID = 103
    USER_ICON_ID = 104
    USER_LOGIN = 105
    USER_PASSWORD = 106
    CHAT_OPTIONS = 109
    USER_FLAGS = 112
    OPTIONS = 113
    CHAT_ID = 114
    USER_NAME_WITH_INFO = 300
    NO_AGREEMENT = 154
    VERSION = 160
    SERVER_NAME = 162
    CAPABILITIES = 0x01F0
    HOPE_APP_ID = 0x0E01
    HOPE_APP_STRING = 0x0E02
    HOPE_SESSION_KEY = 0x0E03
    HOPE_MAC_ALGORITHM = 0x0E04
    HOPE_SERVER_CIPHER = 0x0EC1
    HOPE_CLIENT_CIPHER = 0x0EC2
    FRIEND_LOGIN = 0x0600
    FRIEND_NICKNAME = 0x0601
    PRESENCE_STATE = 0x0602
    PRESENCE_STATUS_TEXT = 0x0603
    ROSTER_STATE = 0x0604
    MESSAGE_GUID = 0x0605
    MESSAGE_BODY = 0x0606
    MESSAGE_TIMESTAMP = 0x0607
    ACK_TYPE = 0x0608
    TYPING_STATE = 0x0609
    REASON_CODE = 0x060F
    REQUEST_NOTE = 0x0610
    DISCOVERABLE = 0x0611
    MAX_MESSAGE_BYTES = 0x0620


CAP_TEXT_ENCODING = 1 << 1
CAP_MESSAGING = 1 << 6
CAP_MESSENGER_SESSION = 1 << 8

ONLINE, AWAY, INVISIBLE, BUSY = 1, 2, 3, 4
ACCEPTED, PENDING_IN = 3, 2
OFFLINE_QUEUED = 7

AEAD_CIPHER = "CHACHA20-POLY1305"
MACS = ["HMAC-SHA256", "HMAC-SHA1", "INVERSE"]


class HotlineError(Exception):
    """A reply with a non-zero error code, or a failed sign-on."""

    def __init__(self, text: str, reason: int | None = None):
        super().__init__(text)
        self.reason = reason


# ---------- transactions ----------

@dataclass
class Transaction:
    ty: int
    fields: list[tuple[int, bytes]] = dfield(default_factory=list)
    id: int = 0
    is_reply: bool = False
    error: int = 0
    flags: int = 0

    def get(self, fid: int) -> bytes | None:
        for i, d in self.fields:
            if i == fid:
                return d
        return None

    def uint(self, fid: int) -> int | None:
        d = self.get(fid)
        return None if d is None else int.from_bytes(d[:8], "big")

    def encode(self) -> bytes:
        body = struct.pack(">H", len(self.fields))
        for i, d in self.fields:
            body += struct.pack(">HH", i, len(d)) + d
        head = struct.pack(">BBHIIII", self.flags, int(self.is_reply), self.ty, self.id,
                           self.error, len(body), len(body))
        return head + body

    @staticmethod
    def decode(head: bytes, payload: bytes) -> "Transaction":
        flags, reply, ty, tid, err, _, _ = struct.unpack(">BBHIIII", head)
        t = Transaction(ty, [], tid, bool(reply), err, flags)
        if len(payload) >= 2:
            (count,) = struct.unpack_from(">H", payload)
            p = 2
            for _ in range(count):
                if p + 4 > len(payload):
                    raise ValueError("truncated transaction")
                fid, ln = struct.unpack_from(">HH", payload, p)
                p += 4
                if p + ln > len(payload):
                    raise ValueError("truncated transaction")
                t.fields.append((fid, payload[p:p + ln]))
                p += ln
        return t

    def entries(self) -> list[list[tuple[int, bytes]]]:
        """Entry groups, each opened by FRIEND_LOGIN (guide §5.5)."""
        out: list[list[tuple[int, bytes]]] = []
        for f in self.fields:
            if f[0] == F.FRIEND_LOGIN:
                out.append([f])
            elif out:
                out[-1].append(f)
        return out


def u16(fid: int, v: int) -> tuple[int, bytes]:
    return (fid, struct.pack(">H", v))


def name_list(names: list[str]) -> bytes:
    out = struct.pack(">H", len(names))
    for n in names:
        out += bytes([len(n)]) + n.encode()
    return out


def parse_name_list(data: bytes | None) -> list[str]:
    if not data or len(data) < 2:
        return []
    (count,) = struct.unpack_from(">H", data)
    out, p = [], 2
    for _ in range(count):
        if p >= len(data):
            break
        ln = data[p]
        out.append(data[p + 1:p + 1 + ln].decode("utf-8", "replace"))
        p += 1 + ln
    return out


def invert(b: bytes) -> bytes:
    return bytes(x ^ 0xFF for x in b)


# ---------- HOPE ----------

def mac(alg: str, key: bytes, msg: bytes) -> bytes:
    """HMAC variants key with `key`; INVERSE is the legacy obfuscation of `key`."""
    if alg == "HMAC-SHA256":
        return hmac.new(key, msg, hashlib.sha256).digest()
    if alg == "HMAC-SHA1":
        return hmac.new(key, msg, hashlib.sha1).digest()
    if alg == "HMAC-MD5":
        return hmac.new(key, msg, hashlib.md5).digest()
    if alg == "SHA1":
        return hashlib.sha1(key + msg).digest()
    if alg == "MD5":
        return hashlib.md5(key + msg).digest()
    return invert(key)


def aead_keys(alg: str, password: bytes, session_key: bytes) -> tuple[bytes, bytes]:
    """(encode, decode), named from the server's side: a client writes with decode."""
    pw_mac = mac(alg, password, session_key)
    enc = mac(alg, password, pw_mac)
    dec = mac(alg, password, enc)

    def kdf(ikm: bytes, info: bytes) -> bytes:
        return HKDF(algorithm=hashes.SHA256(), length=32, salt=session_key, info=info).derive(ikm)

    return kdf(enc, b"hope-chacha-encode"), kdf(dec, b"hope-chacha-decode")


class Sealer:
    """One direction of the AEAD transport: nonce = direction byte, 3 zeros, u64 counter."""

    def __init__(self, key: bytes, direction: int):
        self.aead = ChaCha20Poly1305(key)
        self.dir = direction
        self.counter = 0

    def _nonce(self) -> bytes:
        n = bytes([self.dir, 0, 0, 0]) + struct.pack(">Q", self.counter)
        self.counter += 1
        return n

    def seal(self, plain: bytes) -> bytes:
        return self.aead.encrypt(self._nonce(), plain, None)

    def open(self, sealed: bytes) -> bytes:
        return self.aead.decrypt(self._nonce(), sealed, None)


MAX_READ = 16 * 1024 * 1024


class Wire:
    """Whole transactions on a stream, in the clear or sealed in AEAD frames."""

    def __init__(self, r: asyncio.StreamReader, w: asyncio.StreamWriter):
        self.r, self.w = r, w
        self.rx: Sealer | None = None
        self.tx: Sealer | None = None
        self.buf = b""

    async def _fill(self, want: int) -> None:
        while len(self.buf) < want:
            if self.rx is None:
                self.buf += await self.r.readexactly(want - len(self.buf))
            else:
                (ln,) = struct.unpack(">I", await self.r.readexactly(4))
                if not 16 <= ln <= MAX_READ:
                    raise ValueError("bad AEAD frame length")
                self.buf += self.rx.open(await self.r.readexactly(ln))

    def _take(self, n: int) -> bytes:
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    async def read(self) -> Transaction:
        await self._fill(20)
        head = self._take(20)
        total, part = struct.unpack_from(">II", head, 12)
        if total > MAX_READ or part > MAX_READ:
            raise ValueError("transaction too large")
        payload = b""
        while True:
            await self._fill(part)
            payload += self._take(part)
            if len(payload) >= total:
                break
            await self._fill(20)
            (part,) = struct.unpack_from(">I", self._take(20), 16)
            if part == 0:
                raise ValueError("bad transaction part")
        return Transaction.decode(head, payload)

    async def read_first_sealed(self) -> tuple[Transaction, bool]:
        """After switching to AEAD, a refused login may still come back in the clear:
        a plaintext reply opens 00 01, which as a frame length would be absurd."""
        head4 = await self.r.readexactly(4)
        if head4[0] == 0 and head4[1] == 1:
            head = head4 + await self.r.readexactly(16)
            (part,) = struct.unpack_from(">I", head, 16)
            return Transaction.decode(head, await self.r.readexactly(part)), True
        (ln,) = struct.unpack(">I", head4)
        if not 16 <= ln <= MAX_READ:
            raise ValueError("bad AEAD frame length")
        assert self.rx is not None
        self.buf += self.rx.open(await self.r.readexactly(ln))
        return await self.read(), False

    async def write(self, t: Transaction) -> None:
        data = t.encode()
        if self.tx is not None:
            sealed = self.tx.seal(data)
            data = struct.pack(">I", len(sealed)) + sealed
        self.w.write(data)
        await self.w.drain()


# ---------- the session ----------

@dataclass
class Message:
    guid: bytes
    sender: str
    body: str
    timestamp: int


@dataclass
class Buddy:
    login: str
    state: int
    presence: int = 0
    name: str | None = None
    status: str | None = None


Handler = Callable[[str, dict], Awaitable[None]]


class Client:
    """A signed-on session. `on_event(kind, data)` gets the notifications: message,
    friend_request, roster_entry, presence, ack, typing, server_message, and for a
    classic session (chat rooms) chat, private, user_changed, user_left.

    `classic=True` signs on the way a Hotline 1.9 client does, for a server's public
    chat: no messaging, and with an empty login and password, as a guest."""

    VERSION = 2001  # SmarterChild's year; HIM is 1997.
    APP_ID = b"SMCH"

    def __init__(self, host: str, port: int, login: str, password: str, *,
                 nickname: str | None = None, app_string: str = "SmarterChild",
                 on_event: Handler | None = None, classic: bool = False, icon: int = 0):
        self.host, self.port = host, port
        self.login, self.password = login, password
        self.nickname = nickname or login
        self.app_string = app_string
        self.on_event = on_event
        self.classic, self.icon = classic, icon
        self.users: dict[int, str] = {}  # classic: user ID -> name
        self.icons: dict[int, int] = {}  # classic: user ID -> icon
        self.user_id = 0
        self.utf8 = True
        self.max_message_bytes = 4096
        self.server_name: str | None = None
        self.transport = ""
        self.wire: Wire | None = None
        self.pending: dict[int, asyncio.Future] = {}
        self.next_id = 1
        self.reader_task: asyncio.Task | None = None
        self.closed = asyncio.Event()
        self.close_reason = ""
        self.early: list[Transaction] = []

    # text
    def enc(self, s: str) -> bytes:
        return s.encode("utf-8" if self.utf8 else "mac_roman", "replace")

    def dec(self, b: bytes | None) -> str:
        if b is None:
            return ""
        if not self.utf8:
            try:
                return b.decode("utf-8")
            except UnicodeDecodeError:
                return b.decode("mac_roman", "replace")
        return b.decode("utf-8", "replace")

    # ---- connecting ----

    async def _open(self) -> Wire:
        r, w = await asyncio.wait_for(asyncio.open_connection(self.host, self.port), 12)
        w.write(b"TRTPHOTL\x00\x01\x00\x00")
        await w.drain()
        reply = await asyncio.wait_for(r.readexactly(8), 12)
        if reply[:4] != b"TRTP" or reply[4:] != b"\0\0\0\0":
            w.close()
            raise HotlineError("That isn't a Hotline server.")
        return Wire(r, w)

    async def _call_raw(self, wire: Wire, t: Transaction) -> Transaction:
        t.id = self.next_id
        self.next_id += 1
        await wire.write(t)
        return await self._wait_raw(wire, t.id)

    async def _wait_raw(self, wire: Wire, tid: int) -> Transaction:
        while True:
            r = await asyncio.wait_for(wire.read(), 25)
            if r.is_reply and r.id == tid:
                return r
            if not r.is_reply:
                self.early.append(r)

    def _login_fields(self) -> list[tuple[int, bytes]]:
        return [
            (F.USER_NAME, self.nickname.encode()),
            u16(F.USER_ICON_ID, self.icon),
            # A classic session claims 1.9, which every server knows and still sends the agreement.
            u16(F.VERSION, 190 if self.classic else self.VERSION),
            u16(F.CAPABILITIES, CAP_TEXT_ENCODING if self.classic else
                CAP_MESSAGING | CAP_MESSENGER_SESSION | CAP_TEXT_ENCODING),
        ]

    async def connect(self) -> None:
        """Signs on with HOPE (encrypted when the server agrees), else the legacy login.
        A classic guest skips HOPE: it has no password to protect."""
        wire = await self._open()
        if self.classic and not self.login:
            reply = await self._call_raw(wire, Transaction(Tx.LOGIN, [(F.USER_LOGIN, b""), (F.USER_PASSWORD, b"")]
                                                           + self._login_fields()))
            self.transport = "plaintext (guest)"
            return self._finish(wire, reply)
        step1 = Transaction(Tx.LOGIN, [
            (F.USER_LOGIN, b"\0"), (F.USER_PASSWORD, b"\0"),
            (F.HOPE_MAC_ALGORITHM, name_list(MACS)),
            (F.HOPE_APP_ID, self.APP_ID), (F.HOPE_APP_STRING, self.app_string.encode()),
            (F.HOPE_SESSION_KEY, b""),
            (F.HOPE_CLIENT_CIPHER, name_list([AEAD_CIPHER])),
        ])
        r = await self._call_raw(wire, step1)
        key = r.get(F.HOPE_SESSION_KEY)
        if r.error or not key or len(key) != 64:
            # No HOPE here: the old login, on a fresh connection.
            wire.w.close()
            wire = await self._open()
            fields = [(F.USER_LOGIN, invert(self.login.encode())),
                      (F.USER_PASSWORD, invert(self.password.encode()))] + self._login_fields()
            reply = await self._call_raw(wire, Transaction(Tx.LOGIN, fields))
            self.transport = "plaintext"
            return self._finish(wire, reply)

        algs = parse_name_list(r.get(F.HOPE_MAC_ALGORITHM))
        alg = algs[0].upper() if algs else "INVERSE"
        mac_login = bool(r.get(F.USER_LOGIN))
        norm = lambda c: AEAD_CIPHER if c.upper() in ("CHACHA20", "CHACHA20POLY1305", AEAD_CIPHER) else c.upper()
        sc = [norm(c) for c in parse_name_list(r.get(F.HOPE_SERVER_CIPHER))]
        cc = [norm(c) for c in parse_name_list(r.get(F.HOPE_CLIENT_CIPHER))]
        aead = alg != "INVERSE" and sc[:1] == [AEAD_CIPHER] and cc[:1] == [AEAD_CIPHER]

        pw = self.password.encode()
        login = self.login.encode()
        fields = [
            (F.USER_LOGIN, mac(alg, login, key) if mac_login else invert(login)),
            (F.USER_PASSWORD, mac(alg, pw, key)),
        ] + self._login_fields()
        if aead:
            fields.append((F.HOPE_SERVER_CIPHER, name_list([AEAD_CIPHER])))
        t = Transaction(Tx.LOGIN, fields, id=self.next_id)
        self.next_id += 1
        await wire.write(t)
        if aead:
            encode, decode = aead_keys(alg, pw, key)
            wire.tx = Sealer(decode, 0x01)
            wire.rx = Sealer(encode, 0x00)
            first, plain = await asyncio.wait_for(wire.read_first_sealed(), 25)
            if plain:
                return self._finish(wire, first)
            if first.is_reply and first.id == t.id:
                reply = first
            else:
                if not first.is_reply:
                    self.early.append(first)
                reply = await self._wait_raw(wire, t.id)
            self.transport = "HOPE (ChaCha20-Poly1305)"
        else:
            reply = await self._wait_raw(wire, t.id)
            self.transport = f"HOPE ({alg})"
        self._finish(wire, reply)

    def _finish(self, wire: Wire, reply: Transaction) -> None:
        if reply.error:
            wire.w.close()
            raise HotlineError(self.dec(reply.get(F.ERROR)) or "Incorrect login.")
        caps = reply.uint(F.CAPABILITIES) or 0
        self.user_id = reply.uint(F.USER_ID) or 0
        if not self.classic and not caps & CAP_MESSAGING:
            wire.w.close()
            raise HotlineError("Instant messaging isn't available on this server (or for this account).")
        self.utf8 = bool(caps & CAP_TEXT_ENCODING)
        self.max_message_bytes = reply.uint(F.MAX_MESSAGE_BYTES) or 4096
        self.server_name = self.dec(reply.get(F.SERVER_NAME)) or None
        self.wire = wire
        self.closed.clear()
        self.reader_task = asyncio.create_task(self._reader())
        for t in self.early:
            asyncio.create_task(self._dispatch(t))
        self.early = []

    async def _reader(self) -> None:
        assert self.wire is not None
        reason = "The server closed the connection."
        try:
            while True:
                t = await self.wire.read()
                if t.is_reply:
                    f = self.pending.pop(t.id, None)
                    if f and not f.done():
                        f.set_result(t)
                    continue
                if t.ty == Tx.DISCONNECT_MSG:
                    reason = self.dec(t.get(F.DATA)) or "The server disconnected us."
                    break
                await self._dispatch(t)
        except (asyncio.IncompleteReadError, ConnectionError, OSError):
            pass
        except Exception as e:  # a bad frame, a failed seal: the session can't go on
            reason = f"Connection lost: {e}"
        finally:
            self.close_reason = reason
            for f in self.pending.values():
                if not f.done():
                    f.set_exception(HotlineError("Signed off."))
            self.pending.clear()
            self.closed.set()

    async def _emit(self, kind: str, data: dict) -> None:
        if self.on_event:
            try:
                await self.on_event(kind, data)
            except Exception:
                log.exception("event handler failed on %s", kind)

    async def _dispatch(self, t: Transaction) -> None:
        s = lambda fid: self.dec(t.get(fid))
        if t.ty == Tx.IM_DELIVER:
            guid = t.get(F.MESSAGE_GUID)
            if guid is None:
                return
            m = Message(guid, s(F.FRIEND_LOGIN), s(F.MESSAGE_BODY), t.uint(F.MESSAGE_TIMESTAMP) or 0)
            self.ack(m.guid, m.sender, read=False)  # delivered, at once
            await self._emit("message", {"message": m})
        elif t.ty == Tx.FRIEND_REQUEST:
            await self._emit("friend_request", {"login": s(F.FRIEND_LOGIN), "note": s(F.REQUEST_NOTE)})
        elif t.ty == Tx.ROSTER_ENTRY:
            for g in t.entries():
                await self._emit("roster_entry", {"buddy": self._buddy(g)})
        elif t.ty == Tx.PRESENCE_CHANGED:
            await self._emit("presence", {"login": s(F.FRIEND_LOGIN), "presence": t.uint(F.PRESENCE_STATE) or 0,
                                          "status": s(F.PRESENCE_STATUS_TEXT)})
        elif t.ty == Tx.IM_TYPING:
            await self._emit("typing", {"login": s(F.FRIEND_LOGIN), "typing": t.uint(F.TYPING_STATE) == 1})
        elif t.ty == Tx.IM_ACK:
            await self._emit("ack", {"login": s(F.FRIEND_LOGIN), "guid": t.get(F.MESSAGE_GUID),
                                     "read": t.uint(F.ACK_TYPE) == 2})
        elif t.ty == Tx.SHOW_AGREEMENT:
            # Agreements are accepted without reading them, as HIM does.
            self.notify(Tx.AGREED, [(F.USER_NAME, self.enc(self.nickname)), u16(F.USER_ICON_ID, self.icon),
                                    u16(F.OPTIONS, 0)])
        elif t.ty == Tx.SERVER_MSG:
            uid = t.uint(F.USER_ID)
            if uid is not None:
                await self._emit("private", {"id": uid, "name": s(F.USER_NAME) or self.users.get(uid, ""),
                                             "text": s(F.DATA)})
            else:
                await self._emit("server_message", {"text": s(F.DATA)})
        elif t.ty == Tx.CHAT_MSG:
            await self._emit("chat", {"text": s(F.DATA), "chat_id": t.uint(F.CHAT_ID)})
        elif t.ty == Tx.NOTIFY_CHANGE_USER:
            uid = t.uint(F.USER_ID)
            if uid is not None:
                old = self.users.get(uid)
                self.users[uid] = s(F.USER_NAME).strip()
                self.icons[uid] = t.uint(F.USER_ICON_ID) or 0
                await self._emit("user_changed", {"id": uid, "name": self.users[uid], "icon": self.icons[uid],
                                                  "old_name": old})
        elif t.ty == Tx.NOTIFY_DELETE_USER:
            uid = t.uint(F.USER_ID)
            if uid is not None:
                self.icons.pop(uid, None)
                await self._emit("user_left", {"id": uid, "name": self.users.pop(uid, "")})

    def _buddy(self, group: list[tuple[int, bytes]]) -> Buddy:
        d = dict(group[1:])
        b = Buddy(self.dec(group[0][1]), int.from_bytes(d.get(F.ROSTER_STATE, b"\0"), "big"))
        b.presence = int.from_bytes(d.get(F.PRESENCE_STATE, b"\0"), "big")
        b.name = self.dec(d.get(F.USER_NAME)) or None
        b.status = self.dec(d.get(F.PRESENCE_STATUS_TEXT)) or None
        return b

    # ---- requests ----

    async def request(self, ty: int, fields: list[tuple[int, bytes]]) -> Transaction:
        if self.wire is None or self.closed.is_set():
            raise HotlineError("Not signed on.")
        t = Transaction(ty, fields, id=self.next_id)
        self.next_id += 1
        fut = asyncio.get_running_loop().create_future()
        self.pending[t.id] = fut
        await self.wire.write(t)
        try:
            r = await asyncio.wait_for(fut, 25)
        finally:
            self.pending.pop(t.id, None)
        if r.error:
            raise HotlineError(self.dec(r.get(F.ERROR)) or "The server refused.", r.uint(F.REASON_CODE))
        return r

    def notify(self, ty: int, fields: list[tuple[int, bytes]]) -> None:
        """Fire and forget (receipts, typing)."""
        if self.wire is None or self.closed.is_set():
            return
        t = Transaction(ty, fields, id=self.next_id)
        self.next_id += 1
        asyncio.create_task(self._write_quietly(t))

    async def _write_quietly(self, t: Transaction) -> None:
        try:
            assert self.wire is not None
            await self.wire.write(t)
        except Exception:
            pass

    async def set_presence(self, state: int, status: str = "", discoverable: bool | None = None) -> None:
        f = [u16(F.PRESENCE_STATE, state), (F.PRESENCE_STATUS_TEXT, self.enc(status))]
        if discoverable is not None:
            f.append(u16(F.DISCOVERABLE, int(discoverable)))
        await self.request(Tx.SET_PRESENCE, f)

    async def get_roster(self) -> list[Buddy]:
        r = await self.request(Tx.GET_ROSTER, [])
        return [self._buddy(g) for g in r.entries()]

    async def accept(self, login: str) -> None:
        await self.request(Tx.FRIEND_RESPONSE, [(F.FRIEND_LOGIN, self.enc(login)), u16(F.ROSTER_STATE, ACCEPTED)])

    async def add_friend(self, login: str, note: str = "") -> None:
        f = [(F.FRIEND_LOGIN, self.enc(login))]
        if note:
            f.append((F.REQUEST_NOTE, self.enc(note)))
        await self.request(Tx.ADD_FRIEND, f)

    async def send_im(self, to: str, body: str) -> int:
        """Returns the reason code: 0, or 7 when the server queued it for an offline friend."""
        guid = bytearray(os.urandom(16))
        guid[6] = (guid[6] & 0x0F) | 0x40
        guid[8] = (guid[8] & 0x3F) | 0x80
        r = await self.request(Tx.IM_SEND, [(F.FRIEND_LOGIN, self.enc(to)), (F.MESSAGE_GUID, bytes(guid)),
                                            (F.MESSAGE_BODY, self.enc(body))])
        return r.uint(F.REASON_CODE) or 0

    def ack(self, guid: bytes, login: str, read: bool) -> None:
        self.notify(Tx.IM_ACK, [(F.MESSAGE_GUID, guid), u16(F.ACK_TYPE, 2 if read else 1),
                                (F.FRIEND_LOGIN, self.enc(login))])

    def typing(self, login: str, on: bool) -> None:
        self.notify(Tx.IM_TYPING, [(F.FRIEND_LOGIN, self.enc(login)), u16(F.TYPING_STATE, int(on))])

    # ---- classic chat ----

    def send_chat(self, text: str, emote: bool = False) -> None:
        f = [(F.DATA, self.enc(text))]
        if emote:
            f.append(u16(F.CHAT_OPTIONS, 1))
        self.notify(Tx.SEND_CHAT, f)

    def send_private(self, user_id: int, text: str) -> None:
        self.notify(Tx.SEND_INSTANT_MSG, [u16(F.USER_ID, user_id), u16(F.OPTIONS, 1), (F.DATA, self.enc(text))])

    async def get_users(self) -> dict[int, str]:
        """Everyone on the server: `id(2) icon(2) flags(2) name_len(2) name` per entry."""
        r = await self.request(Tx.GET_USER_NAME_LIST, [])
        self.users, self.icons = {}, {}
        for fid, d in r.fields:
            if fid == F.USER_NAME_WITH_INFO and len(d) >= 8:
                uid, icon, _, ln = struct.unpack_from(">HHHH", d)
                self.users[uid] = self.dec(d[8:8 + ln]).strip()
                self.icons[uid] = icon
        return self.users

    async def close(self) -> None:
        if self.wire is not None:
            self.wire.w.close()
        if self.reader_task:
            await asyncio.wait([self.reader_task], timeout=2)
