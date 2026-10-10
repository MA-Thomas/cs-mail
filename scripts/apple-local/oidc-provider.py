#!/usr/bin/env python3
"""Controlled loopback OIDC fixture, NOT an identity provider for real users.

Exercises code flow, single-use codes, S256, nonce and real RS256 verification by
identity-model. All identities are fictional; no password or real login is tested.
OAuth transactions live only in memory. No request URLs/tokens are logged.
"""
import argparse
import base64
import hashlib
import hmac
import html
import json
import os
import secrets
import subprocess
import tempfile
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock
from urllib.parse import parse_qs, urlencode, urlparse


def b64(value):
    return base64.urlsafe_b64encode(value).decode().rstrip("=")


def one(values, name):
    value = values.get(name, [])
    if len(value) != 1 or not value[0]:
        raise ValueError("invalid parameter")
    return value[0]


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def respond(self, status, body, content_type="application/json", location=None):
        data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Pragma", "no-cache")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("Content-Security-Policy", "default-src 'none'; form-action 'self'; frame-ancestors 'none'")
        if location:
            self.send_header("Location", location)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        try:
            route = urlparse(self.path)
            if route.path == "/.well-known/openid-configuration":
                self.respond(200, {"issuer": self.server.issuer,
                    "authorization_endpoint": self.server.issuer + "/authorize",
                    "token_endpoint": self.server.issuer + "/token",
                    "jwks_uri": self.server.issuer + "/jwks",
                    "response_types_supported": ["code"], "subject_types_supported": ["public"],
                    "id_token_signing_alg_values_supported": ["RS256"],
                    "code_challenge_methods_supported": ["S256"]})
            elif route.path == "/jwks":
                self.respond(200, {"keys": [self.server.jwk]})
            elif route.path == "/authorize":
                query = parse_qs(route.query, strict_parsing=True)
                expected = {"client_id": "cs-mail-apple", "redirect_uri": "com.csqd.member:/oauth/callback",
                    "response_type": "code", "scope": "openid", "code_challenge_method": "S256", "max_age": "0"}
                if any(one(query, k) != v for k, v in expected.items()):
                    raise ValueError("unsupported request")
                record = {k: one(query, k) for k in ["nonce", "state", "code_challenge", "redirect_uri"]}
                record["expires"] = time.time() + 300
                handle = secrets.token_urlsafe(32)
                with self.server.lock:
                    self.server.pending = {k:v for k,v in self.server.pending.items() if v["expires"] > time.time()}
                    if len(self.server.pending) >= 100:
                        raise ValueError("busy")
                    self.server.pending[handle] = record
                self.respond(200, '<!doctype html><meta charset="utf-8"><title>cs-mail test sign-in</title>'
                    '<h1>Local test sign-in</h1><p>Fictional users only. This does not verify a real person.</p>'
                    '<form method="post" action="/approve"><input type="hidden" name="handle" value="' + html.escape(handle) + '">'
                    '<label>Test login <select name="subject"><option value="alice">Alice</option><option value="bob">Bob</option></select></label>'
                    '<button type="submit">Sign in as test user</button></form>', 'text/html; charset=utf-8')
            else:
                self.respond(404, {"error": "not_found"})
        except (ValueError, KeyError):
            self.respond(400, {"error": "invalid_request"})

    def do_POST(self):
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if size <= 0 or size > 16384:
                raise ValueError("invalid body")
            form = parse_qs(self.rfile.read(size).decode(), strict_parsing=True)
            if self.path == "/approve":
                subject = one(form, "subject")
                if subject not in ("alice", "bob"):
                    raise ValueError("invalid user")
                with self.server.lock:
                    record = self.server.pending.pop(one(form, "handle"))
                    if record["expires"] <= time.time():
                        raise ValueError("expired")
                    record["subject"] = subject
                    record["auth_time"] = int(time.time())
                    code = secrets.token_urlsafe(32)
                    self.server.codes = {k:v for k,v in self.server.codes.items() if v["expires"] > time.time()}
                    self.server.codes[code] = record
                callback = record["redirect_uri"] + "?" + urlencode({"code": code, "state": record["state"], "iss": self.server.issuer})
                self.respond(303, "", "text/plain", callback)
            elif self.path == "/token":
                if one(form, "grant_type") != "authorization_code" or one(form, "client_id") != "cs-mail-apple":
                    raise ValueError("invalid client")
                with self.server.lock:
                    record = self.server.codes.pop(one(form, "code"))
                if record["expires"] <= time.time() or one(form, "redirect_uri") != record["redirect_uri"]:
                    raise ValueError("invalid code")
                challenge = b64(hashlib.sha256(one(form, "code_verifier").encode()).digest())
                if not hmac.compare_digest(challenge, record["code_challenge"]):
                    raise ValueError("invalid verifier")
                now = int(time.time())
                claims = {"iss":self.server.issuer, "sub":record["subject"], "aud":"cs-mail-apple", "azp":"cs-mail-apple",
                    "sid":secrets.token_urlsafe(16), "nonce":record["nonce"], "auth_time":record["auth_time"], "iat":now, "exp":now+120}
                signing = (b64(b'{"alg":"RS256","kid":"local-test","typ":"JWT"}') + "." + b64(json.dumps(claims).encode())).encode()
                signature = subprocess.run(["openssl", "dgst", "-sha256", "-sign", self.server.key], input=signing, capture_output=True, check=True).stdout
                self.respond(200, {"id_token":signing.decode()+"."+b64(signature), "token_type":"Bearer", "access_token":secrets.token_urlsafe(32), "expires_in":120})
            else:
                self.respond(404, {"error":"not_found"})
        except (ValueError, KeyError, UnicodeError, subprocess.CalledProcessError):
            self.respond(400, {"error":"invalid_grant"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8482)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="cs-mail-local-oidc-") as directory:
        key = os.path.join(directory, "signing.pem")
        subprocess.run(["openssl", "genrsa", "-out", key, "2048"], check=True, capture_output=True)
        os.chmod(key, 0o600)
        modulus = subprocess.run(["openssl", "rsa", "-in", key, "-noout", "-modulus"], check=True, capture_output=True).stdout.decode().strip().split("=", 1)[1]
        server = ThreadingHTTPServer(("127.0.0.1", args.port), Provider)
        server.key = key
        server.jwk = {"kty":"RSA", "kid":"local-test", "use":"sig", "alg":"RS256", "n":b64(bytes.fromhex(modulus)), "e":"AQAB"}
        server.issuer = f"http://127.0.0.1:{args.port}"
        server.pending, server.codes, server.lock = {}, {}, Lock()
        print(f"Controlled fictional-login provider on {server.issuer}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            server.server_close()


if __name__ == "__main__":
    main()
