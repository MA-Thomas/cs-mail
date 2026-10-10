// The guest request page. It holds no domain rules: the service decides everything and
// this page shows the guest where their request stands. Identifiers are 128-bit
// integers, which JavaScript numbers cannot hold, so they travel as decimal text.
"use strict";

const recipient = (location.pathname.match(/^\/r\/(\d+)\/?$/) || [])[1];
const $ = (id) => document.getElementById(id);
const steps = ["step-mailbox", "step-code", "step-compose", "step-status"];

// Per-tab memory, so a reload resumes; the page works without it.
const memory = {
  get(key) { try { return sessionStorage.getItem(key); } catch { return null; } },
  set(key, value) { try { sessionStorage.setItem(key, value); } catch { /* unavailable */ } },
  clear(key) { try { sessionStorage.removeItem(key); } catch { /* unavailable */ } },
};
const key = (name) => `cs-mail/${recipient}/${name}`;

const messages = {
  "invalid-mailbox": "That email address is not valid.",
  "wrong-code": "That code is not right. Check the latest email and try again.",
  "verification-closed": "That code has expired. Ask for a new one.",
  "too-many-attempts": "Too many attempts. Ask for a new code.",
  "rate-limited": "Too many codes were requested for this address. Try again later.",
  "session-expired": "Your session has ended. Verify your email address again.",
  "pending-request": "You already have a request to this member that has not been decided.",
  "pending-requests-conflict": "You already have requests to this member that have not been decided.",
  "request-in-progress": "A request to this member tied to your email address or card is still being prepared. Finish or cancel it first.",
  "backoff": "You sent a request to this member very recently. Try again a little later.",
  "blocked": "This member does not accept requests from you.",
  "address-taken": "This address cannot be used to write to this member.",
  "unknown-recipient": "This member does not exist.",
  "recipient-not-ready": "This member cannot receive requests yet.",
  "payment-method-declined": "Your card was declined.",
  "invalid-message": "The message is empty or too long.",
  "no-request": "There is no request to act on.",
};

function show(step) {
  for (const id of steps) $(id).hidden = id !== step;
  $("signout").hidden = step === "step-mailbox" || step === "step-code";
}
function fail(text) {
  $("error").textContent = text;
  $("error").hidden = !text;
}
function money(minor) {
  return `${Math.trunc(minor / 100)}.${String(minor % 100).padStart(2, "0")}`;
}
// A fresh 128-bit identifier, as decimal text.
function freshId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return BigInt("0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")).toString();
}
// Parses JSON, keeping integers too large for a number as decimal text.
function parse(text) {
  let out = "";
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inString) {
      out += c;
      if (c === "\\") out += text[++i];
      else if (c === '"') inString = false;
    } else if (c === '"') {
      inString = true;
      out += c;
    } else if (c >= "0" && c <= "9") {
      let j = i;
      while (j < text.length && /[0-9.eE+-]/.test(text[j])) j++;
      const literal = text.slice(i, j);
      out += /^\d{16,}$/.test(literal) ? `"${literal}"` : literal;
      i = j - 1;
    } else {
      out += c;
    }
  }
  return JSON.parse(out);
}
// `body` is JSON text: identifiers are spliced in as bare decimal literals.
async function call(method, path, body) {
  const response = await fetch(path, {
    method,
    headers: body ? { "content-type": "application/json" } : {},
    body,
    cache: "no-store",
  });
  const answer = parse(await response.text() || "null");
  if (!response.ok) {
    const code = answer && answer.error;
    if (code === "session-expired") signOut();
    throw new Error(messages[code] || "Something went wrong. Please try again.");
  }
  return answer;
}
const text = (value) => JSON.stringify(String(value));

async function busy(button, action) {
  fail("");
  button.disabled = true;
  try {
    await action();
  } catch (error) {
    fail(error.message);
  } finally {
    button.disabled = false;
  }
}

// 1. Mailbox and code.
$("form-mailbox").addEventListener("submit", (event) => {
  event.preventDefault();
  busy(event.submitter, async () => {
    const mailbox = $("mailbox").value.trim();
    const { verification } = await call("POST", "/v1/guest/verifications",
      `{"mailbox":${text(mailbox)}}`);
    memory.set(key("verification"), verification);
    $("code-mailbox").textContent = mailbox;
    $("code").value = "";
    show("step-code");
    $("code").focus();
  });
});
$("form-code").addEventListener("submit", (event) => {
  event.preventDefault();
  busy(event.submitter, async () => {
    const verification = memory.get(key("verification"));
    const { session, mailbox } = await call("POST",
      `/v1/guest/verifications/${encodeURIComponent(verification)}/confirm`,
      `{"code":${text($("code").value.trim())}}`);
    memory.set(key("session"), session);
    memory.set(key("mailbox"), mailbox);
    memory.clear(key("verification"));
    await resume();
  });
});
function signOut() {
  for (const name of ["session", "mailbox", "operation", "request", "verification"]) memory.clear(key(name));
  stopPolling();
  show("step-mailbox");
}
$("signout").addEventListener("click", signOut);

// 2. Composing: the member's classes, the charge and the disclosures.
async function compose() {
  const offer = await call("GET", `/v1/guest/recipients/${recipient}/offer`);
  $("compose-mailbox").textContent = memory.get(key("mailbox"));
  const fieldset = $("classes");
  fieldset.querySelectorAll("label").forEach((label) => label.remove());
  offer.offer.classes.forEach((choice, index) => {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = "class";
    input.value = String(choice.selection.id);
    input.required = true;
    input.checked = index === 0;
    const name = document.createElement("span");
    name.textContent = choice.selection.description;
    const price = document.createElement("span");
    price.className = "price";
    price.textContent = money(offer.offer.processing_charge + choice.collateral);
    label.append(input, name, price);
    fieldset.append(label);
  });
  const charge = offer.offer.processing_charge;
  const days = (ms) => Math.max(1, Math.round(ms / 86400000));
  const decision = offer.decision_window < 86400000
    ? `${Math.round(offer.decision_window / 1000)} seconds`
    : `${days(offer.decision_window)} days`;
  $("terms-summary").textContent =
    `Your card is charged the amount shown once, before you send. If the recipient accepts, or you cancel ` +
    `before the request is sent, all of it is refunded to the same card. If they reject it, ` +
    `none is refunded. If they do not decide within ${decision}, all but the processing ` +
    `charge of ${money(charge)} is refunded.`;
  $("not-ready").hidden = offer.ready;
  $("send").disabled = !offer.ready;
  show("step-compose");
}
$("form-request").addEventListener("submit", (event) => {
  event.preventDefault();
  busy(event.submitter, async () => {
    const operation = memory.get(key("operation")) || freshId();
    memory.set(key("operation"), operation);
    const choice = new FormData(event.target).get("class");
    const answer = await call("POST", "/v1/guest/requests",
      `{"session":${text(memory.get(key("session")))},"recipient":${recipient},` +
      `"class":${choice},"instrument":${text($("card").value.replace(/[\s-]/g, ""))},` +
      `"message":${text($("message").value)},"operation":${operation}}`);
    memory.set(key("request"), String(answer.request));
    $("card").value = "";
    $("message").value = "";
    $("agree").checked = false;
    await follow();
  });
});

// 3. Following the request to its outcome.
let timer = null;
function stopPolling() {
  if (timer) clearTimeout(timer);
  timer = null;
}
function step(path) {
  return call("POST", path,
    `{"session":${text(memory.get(key("session")))},"recipient":${recipient},` +
    `"operation":${memory.get(key("operation"))}}`);
}
async function status() {
  return call("POST", "/v1/guest/status",
    `{"session":${text(memory.get(key("session")))},"recipient":${recipient}}`);
}
// This page's request: the one it created, by identifier.
function latest(view) {
  const id = memory.get(key("request"));
  return (view && id && view.requests.find((request) => String(request.id) === id)) || null;
}
function stage(request) {
  return Object.keys(request.lifecycle)[0] || String(request.lifecycle);
}
const terminal = new Set(["Accepted", "Rejected", "Expired", "Cancelled"]);

function render(request) {
  const funds = request.money;
  const where = stage(request);
  const line = {
    PreparingSubmission: funds && funds.capture === "FundingFinalized"
      ? `Your card was charged ${money(funds.charge)}. Send the request, or cancel for a full refund.`
      : "Authorizing the charge on your card…",
    AwaitingRecipientDecision: "Your request was delivered. The recipient has not decided yet.",
    Accepted: "The recipient accepted your request.",
    Rejected: "The recipient rejected your request.",
    Expired: "The recipient did not decide in time.",
    Cancelled: "You cancelled your request before it was sent.",
  }[where] || where;
  let deadline = "";
  if (where === "AwaitingRecipientDecision") {
    deadline = ` They decide by ${new Date(Number(request.lifecycle[where].decision_deadline)).toLocaleString()}.`;
  }
  $("status-line").textContent = line + deadline;

  const rows = [];
  if (funds) {
    rows.push(["Charged", money(funds.charge)]);
    const refund = funds.refund;
    if (refund && refund.Owed !== undefined) rows.push(["Refund to your card", `${money(refund.Owed)} (on its way)`]);
    if (refund && refund.Confirmed !== undefined) rows.push(["Refunded to your card", money(refund.Confirmed)]);
    if (funds.processing_retained) rows.push(["Processing charge kept", money(funds.processing_retained)]);
    if (funds.collateral_forfeited) rows.push(["Collateral forfeited", money(funds.collateral_forfeited)]);
  }
  const list = $("money");
  list.replaceChildren(...rows.flatMap(([term, value]) => {
    const dt = document.createElement("dt");
    dt.textContent = term;
    const dd = document.createElement("dd");
    dd.textContent = value;
    return [dt, dd];
  }));
  list.hidden = rows.length === 0;
  $("status-line").dataset.stage = where;
  $("submit").hidden = !(where === "PreparingSubmission" && funds && funds.capture === "FundingFinalized");
  $("cancel").hidden = where !== "PreparingSubmission";
  $("again").hidden = !terminal.has(where);
}

async function follow() {
  stopPolling();
  show("step-status");
  let request;
  try {
    request = latest(await status());
    if (!request) return compose();
    render(request);
    fail("");
  } catch (error) {
    fail(error.message);
    if (!memory.get(key("session"))) return;
  }
  const settled = request && terminal.has(stage(request)) && request.money &&
    !request.money.awaiting_processor && !(request.money.refund && request.money.refund.Owed !== undefined);
  const choosing = request && !$("submit").hidden;
  if (!settled && !choosing) timer = setTimeout(follow, 1500);
}
$("submit").addEventListener("click", (event) => {
  busy(event.currentTarget, async () => {
    stopPolling();
    render(latest(await step("/v1/guest/requests/submit")));
    timer = setTimeout(follow, 1500);
  });
});
$("cancel").addEventListener("click", (event) => {
  busy(event.currentTarget, async () => {
    stopPolling();
    render(latest(await step("/v1/guest/requests/cancel")));
    timer = setTimeout(follow, 1500);
  });
});
$("again").addEventListener("click", (event) => {
  busy(event.currentTarget, async () => {
    stopPolling();
    memory.clear(key("operation"));
    memory.clear(key("request"));
    await compose();
  });
});

async function resume() {
  if (!recipient) {
    fail("This page needs a member's address.");
    return;
  }
  if (!memory.get(key("session"))) {
    show(memory.get(key("verification")) ? "step-code" : "step-mailbox");
    return;
  }
  try {
    await (memory.get(key("request")) ? follow() : compose());
  } catch (error) {
    fail(error.message);
  }
}
resume();
