#!/usr/bin/env python3
"""Serve the repository the way GitHub Pages does and fetch every internal
reference from the two published pages.

This exists because the landing page and the demo are checked in as built
artefacts, and nothing verified that the paths inside them still resolve. A
Vite base-path change or a renamed asset would break the published site while
every test stayed green.

No browser needed: it starts an HTTP server on a free port, reads the pages,
and fetches what they point at.
"""

import functools
import http.server
import os
import re
import socketserver
import sys
import threading
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAGES = ["index.html", "demo/index.html"]


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


class Server(socketserver.TCPServer):
    # Requests still in flight when the server stops are expected; their
    # tracebacks are noise in a CI log.
    def handle_error(self, request, client_address):
        pass


def get(base, path):
    try:
        with urllib.request.urlopen(base + path, timeout=10) as response:
            return response.status
    except urllib.error.HTTPError as exc:
        return exc.code
    except Exception:
        return None


def main():
    handler = functools.partial(Quiet, directory=ROOT)
    httpd = Server(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    base = "http://127.0.0.1:{}/".format(port)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    problems = []
    checked = 0
    seen = set()

    try:
        for page in PAGES:
            path = os.path.join(ROOT, page)
            if not os.path.exists(path):
                problems.append((page, "-", "page is missing"))
                continue
            if get(base, page) != 200:
                problems.append((page, "-", "page does not serve"))
                continue

            text = open(path, encoding="utf-8", errors="replace").read()
            refs = re.findall(r'(?:href|src)="([^"]+)"', text)
            # Social preview images are referenced from meta tags, not from
            # src, and a missing one is invisible until someone shares the link.
            for tag in re.findall(r"<meta[^>]*>", text):
                if "og:image" in tag or "twitter:image" in tag:
                    found = re.search(r'content="([^"]+)"', tag)
                    if found:
                        refs.append(found.group(1))
            for ref in refs:
                if ref.startswith(("http://", "https://", "mailto:", "#", "data:")):
                    continue
                target = urllib.parse.urljoin(page, ref.split("#")[0])
                if not target or target in seen:
                    continue
                seen.add(target)
                checked += 1
                status = get(base, target)
                if status != 200:
                    problems.append((page, ref, "HTTP {}".format(status)))
    finally:
        httpd.shutdown()

    if problems:
        print("broken references in the published pages:\n")
        for page, ref, why in problems:
            print("  {}\n    {}\n    -> {}\n".format(page, ref, why))
        print("{} problem(s)".format(len(problems)))
        return 1

    print("{} pages, {} internal references, all serve".format(len(PAGES), checked))
    return 0


if __name__ == "__main__":
    sys.exit(main())
