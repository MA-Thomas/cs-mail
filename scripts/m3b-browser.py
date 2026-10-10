"""Milestone 3b demonstration, browser half: guests use the request page in headless
Chromium and the recipient decides with the developer CLI. Run by scripts/m3b-demo.sh,
which sets the environment below. Needs Python Playwright and a Chromium it can find.

  CS_DEMO_BASE      the service's address, e.g. http://127.0.0.1:8480
  CS_DEMO_BOB       the recipient's persona
  CS_DEMO_OUTBOX    the development outbox directory
  CS_DEMO_CLI       the cs-mail-cli binary
  CS_DEMO_BOB_HOME  the recipient's CLI home
  CS_DEMO_SHOTS     where screenshots are written
"""
import os
import re
import subprocess
import sys
import time
from pathlib import Path

from playwright.sync_api import expect, sync_playwright

BASE = os.environ["CS_DEMO_BASE"]
BOB = os.environ["CS_DEMO_BOB"]
OUTBOX = Path(os.environ["CS_DEMO_OUTBOX"])
CLI = os.environ["CS_DEMO_CLI"]
BOB_HOME = os.environ["CS_DEMO_BOB_HOME"]
SHOTS = Path(os.environ["CS_DEMO_SHOTS"])
SHOTS.mkdir(parents=True, exist_ok=True)


def step(text):
    print(f"\n==> {text}", flush=True)


def say(text):
    print(f"--- {text}", flush=True)


def bob(*args):
    print(f"\n$ Bob: cs-mail-cli {' '.join(args)}", flush=True)
    out = subprocess.run([CLI, "--home", BOB_HOME, *args], capture_output=True, text=True)
    print(out.stdout, end="", flush=True)
    if out.returncode != 0:
        print(out.stderr, end="", file=sys.stderr)
        raise SystemExit(f"cs-mail-cli {args[0]} failed")
    return out.stdout


def guest_identity(mailbox):
    """The guest's sender identity, as Bob's inbox shows it."""
    inbox = subprocess.run([CLI, "--home", BOB_HOME, "inbox"], capture_output=True, text=True).stdout
    match = re.search(rf"^from (\d+) \(guest {re.escape(mailbox)}\)", inbox, re.M | re.I)
    if not match:
        raise SystemExit(f"{mailbox} is not in Bob's inbox")
    return match.group(1)


def latest_mail(to, subject, since):
    """The newest outbox message to `to` with `subject`, written after `since`."""
    for _ in range(100):
        found = []
        for path in OUTBOX.glob("*.eml"):
            text = path.read_text()
            if (path.stat().st_mtime >= since and re.search(rf"^To: {re.escape(to)}\r?$", text, re.M | re.I)
                    and re.search(rf"^Subject: {re.escape(subject)}", text, re.M)):
                found.append((path.stat().st_mtime, text))
        if found:
            return max(found)[1]
        time.sleep(0.1)
    raise SystemExit(f"no '{subject}' mail to {to}")


class Guest:
    def __init__(self, browser, mailbox, name):
        self.mailbox = mailbox
        self.name = name
        self.context = browser.new_context()
        self.page = self.context.new_page()
        self.shot = 0

    def snap(self, label):
        self.shot += 1
        self.page.screenshot(path=SHOTS / f"{self.name}-{self.shot}-{label}.png", full_page=True)

    def verify(self):
        page = self.page
        page.goto(f"{BASE}/r/{BOB}")
        since = time.time() - 1
        page.fill("#mailbox", self.mailbox)
        page.click("#form-mailbox button")
        expect(page.locator("#step-code")).to_be_visible()
        code = re.search(r"code is (\d{6})", latest_mail(self.mailbox, "Your cs-mail verification code", since)).group(1)
        say(f"{self.mailbox}: verification code read from the outbox")
        page.fill("#code", code)
        page.click("#form-code button")
        expect(page.locator("#step-compose")).to_be_visible()
        return self

    def compose(self, klass, message, card):
        page = self.page
        page.check(f"#classes input[value='{klass}']")
        page.fill("#message", message)
        page.fill("#card", card)
        page.check("#agree")
        self.snap("compose")
        page.click("#send")

    def charged(self):
        expect(self.page.locator("#submit")).to_be_visible(timeout=20000)
        self.snap("charged")
        self.report()

    def error(self):
        expect(self.page.locator("#error")).to_be_visible(timeout=20000)
        text = self.page.locator("#error").inner_text()
        say(f"{self.mailbox}: page says: {text}")
        self.snap("refused")
        return text

    def submit(self):
        self.page.click("#submit")
        self.wait("AwaitingRecipientDecision")

    def cancel(self):
        self.page.click("#cancel")
        self.wait("Cancelled")

    def wait(self, stage, timeout=20000):
        expect(self.page.locator(f"#status-line[data-stage='{stage}']")).to_be_attached(timeout=timeout)

    def settled(self, stage, timeout=20000):
        """Waits for the outcome and the processor's confirmation of any refund."""
        self.wait(stage, timeout)
        deadline = time.time() + 30
        while "on its way" in self.page.locator("#money").inner_text() and time.time() < deadline:
            time.sleep(0.3)
        self.snap(stage.lower())
        self.report()

    def report(self):
        say(f"{self.mailbox}: {self.page.locator('#status-line').inner_text()}")
        money = self.page.locator("#money").inner_text().split("\n")
        for term, value in zip(money[::2], money[1::2]):
            print(f"      {term}: {value}", flush=True)


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()

        step("Ana verifies her address, pays and sends; Bob accepts: the full charge is refunded to her card")
        ana = Guest(browser, "ana@guest.example", "ana").verify()
        ana.compose(1, "Hello Bob, Ana here. marker-ana-7f3c", "4242 4242 4242 4242")
        ana.charged()
        ana.submit()
        ana.report()
        bob("inbox")
        bob("decide", "--from", guest_identity(ana.mailbox), "--decision", "accept")
        ana.settled("Accepted")

        step("Ben sends a business proposal; Bob rejects: nothing is refunded")
        ben = Guest(browser, "ben@guest.example", "ben").verify()
        ben.compose(2, "A proposal from Ben. marker-ben-19ad", "5555555555554444")
        ben.charged()
        ben.submit()
        bob("decide", "--from", guest_identity(ben.mailbox), "--decision", "reject")
        ben.settled("Rejected")

        step("Dan authorizes the charge and cancels before sending: the full charge is refunded")
        dan = Guest(browser, "dan@guest.example", "dan").verify()
        dan.compose(1, "Never sent. marker-dan-5b21", "6011111111111117")
        dan.charged()
        dan.cancel()
        dan.settled("Cancelled")

        step("Eve's card is declined: no request is made")
        eve = Guest(browser, "eve@guest.example", "eve").verify()
        eve.compose(1, "Declined. marker-eve-c0de", "4000000000000002")
        eve.error()

        step("Cleo sends; Bob does not decide: the request expires and all but the processing charge is refunded")
        cleo = Guest(browser, "cleo@guest.example", "cleo").verify()
        cleo.compose(1, "Are you there? marker-cleo-a11e", "378282246310005")
        cleo.charged()
        cleo.submit()

        step("Cleo tries again under another address with the same card: it shares her history and its backoff")
        alias = Guest(browser, "cleo.alias@guest.example", "cleo-alias").verify()
        alias.compose(1, "Me again. marker-alias-e5e5", "378282246310005")
        refusal = alias.error()
        if "not been decided" not in refusal and "very recently" not in refusal:
            raise SystemExit("a new address with the same card was not tied to Cleo's history")

        say("waiting for the 20 s decision window and the expiry worker")
        cleo.settled("Expired", timeout=90000)

        step("Bob's inbox: guests appear by their verified address")
        bob("inbox")

        step("notices in the development outbox (DKIM-signed)")
        time.sleep(2)
        for path in sorted(OUTBOX.glob("*.eml"), key=lambda p: p.stat().st_mtime):
            text = path.read_text()
            to = re.search(r"^To: (.*)$", text, re.M).group(1).strip()
            subject = re.search(r"^Subject: (.*)$", text, re.M).group(1).strip()
            signed = "signed" if text.startswith("DKIM-Signature:") else "UNSIGNED"
            print(f"  {to:28} {subject} ({signed})")
        browser.close()


if __name__ == "__main__":
    main()
