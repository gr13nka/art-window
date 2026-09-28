"""One paced HTTP client shared by every catalogue source.

Every museum here is reachable over plain HTTPS with no auth, but the Met
sits behind Imperva, which throttles an unfamiliar client hard — this is the
one place that owns identifying honestly (a real User-Agent), waiting between
requests to the same host, and backing off when a host starts refusing
anyway, so no source module has to remember any of that itself.

Also holds `JsonCache`, the on-disk per-object cache that makes a build
resumable: what a source has already learned about one object it never asks
the network for again, which is what turns an interrupted multi-hour Met pass
into a rerun that only chases what's still missing.
"""

import http.client
import http.cookiejar
import json
import os
import ssl
import sys
import time
import urllib.error
import urllib.request
from urllib.parse import urlparse

USER_AGENT = "ArtWindow/0.1 (+https://github.com/gr13nka/art-window)"
REQUEST_TIMEOUT = 60.0

# A body that arrives short, or a connection that resets mid-transfer, is a
# transient network failure distinct from the 403/429 a host answers on
# purpose — `open` below already backs off on those. `PacedClient._read`
# retries these instead, since they surface from `response.read()`, after
# `open` has already succeeded.
TRANSIENT_READ_ERRORS = (http.client.IncompleteRead, ConnectionError, TimeoutError)


class SourceAborted(Exception):
    """A host kept refusing this client after every retry the backoff policy
    allows. Raised so a source's caller can stop that source for this run
    rather than spin forever — the per-object cache means a later rerun picks
    up wherever this one got to."""


def _build_opener(cafile: str | None = None) -> urllib.request.OpenerDirector:
    context = ssl.create_default_context(cafile=cafile)
    return urllib.request.build_opener(
        urllib.request.HTTPSHandler(context=context),
        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
    )


class PacedClient:
    """An opener that waits `gap_seconds[host]` (default `default_gap`
    seconds) between requests to the same host, and backs off exponentially
    on HTTP 403/429 before raising `SourceAborted` after `max_retries`
    attempts.
    """

    def __init__(
        self,
        gap_seconds: dict[str, float] | None = None,
        default_gap: float = 1.0,
        max_retries: int = 6,
        max_backoff: float = 120.0,
        read_retries: int = 4,
    ):
        self.gap_seconds = dict(gap_seconds or {})
        self.default_gap = default_gap
        self.max_retries = max_retries
        self.max_backoff = max_backoff
        self.read_retries = read_retries
        self._last_request_at: dict[str, float] = {}
        self._opener = _build_opener()
        self._cert_fallback_tried = False

    def _wait(self, host: str) -> None:
        gap = self.gap_seconds.get(host, self.default_gap)
        last = self._last_request_at.get(host)
        now = time.monotonic()
        if last is not None and now - last < gap:
            time.sleep(gap - (now - last))
        self._last_request_at[host] = time.monotonic()

    def _retry_with_certifi(self) -> bool:
        # Homebrew's Python on macOS ships no CA bundle of its own, which
        # reads as "self-signed certificate in chain" against a perfectly
        # ordinary host. `certifi`'s bundle is a drop-in fix; if it isn't
        # installed, `open` raises a `RuntimeError` naming the fix rather
        # than silently skipping verification. Tried once per client.
        if self._cert_fallback_tried:
            return False
        self._cert_fallback_tried = True
        try:
            import certifi
        except ImportError:
            return False
        self._opener = _build_opener(cafile=certifi.where())
        print(
            "note: system CA bundle rejected by TLS handshake (self-signed "
            "certificate in chain); falling back to certifi's bundle",
            file=sys.stderr,
        )
        return True

    def open(self, url: str, headers: dict[str, str] | None = None, data: bytes | None = None):
        host = urlparse(url).netloc
        request_headers = {"User-Agent": USER_AGENT}
        request_headers.update(headers or {})
        backoff = 2.0
        attempt = 0
        while True:
            self._wait(host)
            request = urllib.request.Request(url, headers=request_headers, data=data)
            try:
                return self._opener.open(request, timeout=REQUEST_TIMEOUT)
            except urllib.error.URLError as e:
                reason = getattr(e, "reason", None)
                cert_error = isinstance(reason, ssl.SSLCertVerificationError) or (
                    "certificate verify failed" in str(e)
                )
                if cert_error:
                    if self._retry_with_certifi():
                        continue
                    raise RuntimeError(
                        "TLS certificate verification failed (self-signed "
                        "certificate in chain) and the 'certifi' package is not "
                        "installed. Fix with one of:\n"
                        "  pip3 install certifi\n"
                        "  export SSL_CERT_FILE=$(python3 -c "
                        "'import certifi; print(certifi.where())')\n"
                        "(seen on Homebrew Python on macOS, which ships no CA "
                        "bundle of its own)"
                    ) from e
                code = getattr(e, "code", None)
                if code in (403, 429):
                    attempt += 1
                    if attempt > self.max_retries:
                        raise SourceAborted(
                            f"{host} returned HTTP {code} on {self.max_retries} "
                            "consecutive attempts (backed off up to "
                            f"{min(backoff, self.max_backoff):.0f}s) — giving up "
                            "on this source for this run; its per-object cache "
                            "means a rerun resumes rather than starts over"
                        ) from e
                    wait = min(backoff, self.max_backoff)
                    print(
                        f"  {host} -> HTTP {code}, backing off {wait:.0f}s "
                        f"(attempt {attempt}/{self.max_retries})",
                        file=sys.stderr,
                    )
                    time.sleep(wait)
                    backoff *= 2
                    continue
                raise

    def _with_read_retry(self, attempt):
        """Runs `attempt` — a zero-arg callable that opens a fresh connection
        and reads the full response — up to `read_retries` times, retrying
        on `TRANSIENT_READ_ERRORS`. Each retry reopens the connection rather
        than resuming the old one, since a response object can't be re-read
        once its stream has failed partway through."""
        backoff = 2.0
        for attempt_no in range(1, self.read_retries + 1):
            try:
                return attempt()
            except TRANSIENT_READ_ERRORS as e:
                if attempt_no == self.read_retries:
                    raise
                print(
                    f"  short or reset read ({e}), retrying in {backoff:.0f}s "
                    f"(attempt {attempt_no}/{self.read_retries})",
                    file=sys.stderr,
                )
                time.sleep(backoff)
                backoff *= 2

    def get_bytes(self, url: str, headers: dict[str, str] | None = None) -> bytes:
        def _attempt():
            with self.open(url, headers=headers) as response:
                return response.read()

        return self._with_read_retry(_attempt)

    def get_json(self, url: str, headers: dict[str, str] | None = None):
        return json.loads(self.get_bytes(url, headers=headers))

    def get_range(self, url: str, start: int, end: int) -> bytes:
        """Bytes `start..=end` of `url` (HTTP Range, inclusive on both ends).
        Some servers ignore Range and return the whole body instead — callers
        that care should check the length of what comes back."""

        def _attempt():
            with self.open(url, headers={"Range": f"bytes={start}-{end}"}) as response:
                return response.read()

        return self._with_read_retry(_attempt)

    def download(self, url: str, dest_path: str) -> int:
        """Streams `url` to `dest_path` and returns the byte count written."""
        os.makedirs(os.path.dirname(dest_path) or ".", exist_ok=True)
        tmp_path = dest_path + ".part"

        def _attempt():
            with self.open(url) as response, open(tmp_path, "wb") as f:
                while True:
                    chunk = response.read(1 << 16)
                    if not chunk:
                        break
                    f.write(chunk)

        self._with_read_retry(_attempt)
        os.replace(tmp_path, dest_path)
        return os.path.getsize(dest_path)


class JsonCache:
    """One JSON file per object under `directory`, keyed by an id string.
    `put` accepts anything reject-worthy too (a source stores `{"skip":
    "..."}` for a candidate it decided against) so a rerun never re-asks a
    question it already has the answer to."""

    def __init__(self, directory: str):
        self.directory = directory
        os.makedirs(directory, exist_ok=True)

    def _path(self, key: str) -> str:
        return os.path.join(self.directory, f"{key}.json")

    def get(self, key: str):
        try:
            with open(self._path(key), encoding="utf-8") as f:
                return json.load(f)
        except (FileNotFoundError, json.JSONDecodeError):
            return None

    def put(self, key: str, value) -> None:
        path = self._path(key)
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(value, f)
        os.replace(tmp, path)
