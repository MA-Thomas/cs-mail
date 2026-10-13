#!/usr/bin/env python3
"""Provision controlled participants using signed workflows in a persistent local environment."""
import argparse
import base64
import fcntl
import hashlib
import html
import json
import re
import secrets
import subprocess
import time
import urllib.parse
import urllib.request
from pathlib import Path

CALLBACK = "com.csqd.member.development:/oauth/callback"
PARTICIPANTS = [
    ("maya", "Maya Chen", 11),
    ("daniel", "Daniel Reed", 12),
    ("elena", "Elena Park", 13),
    ("jules", "Jules Morgan", 14),
    ("noah", "Noah Ellis", 15),
]


def read_public_profile(server, reference):
    request = urllib.request.Request(
        server + "/v1/directory/lookup",
        json.dumps({"reference": reference}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.load(response)


def fresh_login(issuer, nonce, subject):
    """Use the controlled provider's normal nonce, state and PKCE ceremony."""
    verifier = secrets.token_urlsafe(32)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
    parameters = dict(
        client_id="cs-mail-apple", redirect_uri=CALLBACK, response_type="code", scope="openid",
        code_challenge_method="S256", max_age="0", nonce=nonce,
        state=secrets.token_urlsafe(16), code_challenge=challenge,
    )
    with urllib.request.urlopen(issuer + "/authorize?" + urllib.parse.urlencode(parameters), timeout=10) as response:
        page = response.read().decode()
    handle = html.unescape(re.search(r'name="handle" value="([^"]+)"', page).group(1))
    request = urllib.request.Request(
        issuer + "/approve", urllib.parse.urlencode(dict(handle=handle, subject=subject)).encode(),
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        returned = response.read().decode()
    link = html.unescape(re.search(r'<a href="([^"]+)"', returned).group(1))
    callback = urllib.parse.urlparse(link)
    fields = urllib.parse.parse_qs(callback.query)
    if callback.scheme + ":" + callback.path != CALLBACK or fields["state"] != [parameters["state"]]:
        raise RuntimeError("Invalid controlled login callback")
    if fields["iss"] != [issuer]:
        raise RuntimeError("Controlled login issuer changed")
    request = urllib.request.Request(issuer + "/token", urllib.parse.urlencode(dict(
        grant_type="authorization_code", client_id="cs-mail-apple", redirect_uri=CALLBACK,
        code=fields["code"][0], code_verifier=verifier,
    )).encode())
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.load(response)["id_token"]


def save_private_json(path, value):
    temporary = path.with_suffix(".new")
    temporary.touch(mode=0o600, exist_ok=True)
    temporary.chmod(0o600)
    temporary.write_text(json.dumps(value))
    temporary.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state-dir", type=Path, required=True)
    parser.add_argument("--recipient", required=True, help="Exact published mailbox or protocol address reference")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    environment = json.loads((args.state_dir / "environment.json").read_text())
    server = f"http://127.0.0.1:{environment['ports'][0]}"
    issuer = f"http://127.0.0.1:{environment['ports'][2]}"
    approach = read_public_profile(server, args.recipient)
    if not approach["covered"] or not approach["offer"] or not approach["offer"]["classes"]:
        raise RuntimeError("Recipient needs covered service and a published request class")
    recipient = str(approach["entry"]["address"])
    request_class = str(approach["offer"]["classes"][0]["selection"]["id"])
    directory = args.state_dir / "participants"
    directory.mkdir(mode=0o700, exist_ok=True)
    # Keep the lock alive throughout every CLI mutation of these fixture device stores.
    with (directory / ".owner.lock").open("a") as owner:
        fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
        binding_path = directory / "recipient.json"
        binding = {"recipient": recipient, "server": server}
        if binding_path.exists() and json.loads(binding_path.read_text()) != binding:
            raise RuntimeError("Participant recipient changed; preserve this environment")
        save_private_json(binding_path, binding)
        for subject, name, number in PARTICIPANTS:
            home = directory / subject

            def run(*command):
                result = subprocess.run(
                    [str(root / "target/debug/cs-mail-cli"), "--home", str(home), *command],
                    capture_output=True, text=True, timeout=90,
                )
                if result.returncode:
                    raise RuntimeError(f"{subject}: {command[0]} failed: {result.stderr}")
                return result.stdout

            if not (home / "member.json").exists():
                run("init", "--server", server)
            state = json.loads((home / "member.json").read_text())
            if state["account"] is None:
                applicant = dict(
                    name=name + " Development Fixture", date_of_birth=f"1991-02-{number:02d}",
                    address=f"{number} Fictional Participant Street", phone=f"+1 555 019 {number:04d}",
                )
                bank = dict(account=f"csqd-development-{subject}-simulated-bank",
                            owner={field: applicant[field] for field in ("name", "address", "phone")})
                save_private_json(home / "applicant.json", applicant)
                save_private_json(home / "bank.json", bank)
                begun = json.loads(run("enroll-begin", "--bank", str(home / "bank.json")))
                token = fresh_login(issuer, begun["challenge"], subject)
                run("enroll-complete", "--oidc-token", token, "--login-issuer", issuer,
                    "--login-subject", subject, "--applicant", str(home / "applicant.json"),
                    "--bank", str(home / "bank.json"))
            run("register-recipient-key")
            run("publish-content-key")
            if subject != "noah":
                run("purchase", "--offer", "1")
            run("publish-classes", "--version", "1", "--class", "1=A thoughtful introduction=500")
            publication = home / "publication.complete"
            if not publication.exists():
                run("publish-directory", "--name", name,
                    "--description", "Fictional participant for development interactions.",
                    "--mailbox", subject + "@csqd-mail.com")
                publication.touch(mode=0o600)
            if subject != "noah":
                deadline = time.monotonic() + 30
                while not read_public_profile(server, subject + "@csqd-mail.com")["covered"]:
                    if time.monotonic() > deadline:
                        raise RuntimeError(subject + ": simulated coverage did not become active")
                    time.sleep(.3)
                completed = home / "request.complete"
                if not completed.exists():
                    command = ("request", "--to", recipient, "--class", request_class, "--message",
                               f"Hello Marcus,\n\nThis is a controlled development introduction from {name}. "
                               f"Banking and payments are simulated.\n\n{name}")
                    try:
                        run(*command)
                    except RuntimeError as error:
                        if "ServiceNotCovered" not in str(error):
                            raise
                        # Only a definitively refused terms operation can be started afresh.
                        # The CLI retains its receipt; no charge existed for that operation.
                        run(*command)
                    completed.touch(mode=0o600)
            print(f"{name}: canonical participant ready", flush=True)
    print("Noah has a published profile without purchased coverage; new requests are unavailable.")


if __name__ == "__main__":
    main()
