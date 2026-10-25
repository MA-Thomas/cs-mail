# Correspondence Does Not Grant Permission to Observe

**Information about a recipient’s behavior should be disclosed according to the recipient’s permissions, rather than inferred through hidden instrumentation controlled by the sender.**

Permission to communicate with someone does not confer permission to observe their behavior.

A tracking pixel is only one example of hidden instrumentation. Ordinary images, fonts and other remotely loaded resources can serve the same purpose; recognizing known trackers is insufficient.

**Examining correspondence must not automatically contact external destinations specified by its contents.** This applies to displaying, previewing, searching and quoting messages, including background processing performed on the recipient’s behalf.

Preserve the sender’s content, structure and visual presentation where safely possible. Resources supplied within a message may be displayed locally. Fetching external images, fonts, stylesheets or other resources requires the recipient’s explicit authorization. Authorization must identify the corresponding disclosure and have a defined scope; it must not silently extend to additional requests.

A hyperlink is a reference to a destination, not an instruction to visit it. Static inspection of a URL’s text or structure is distinct from fetching it. Display, search, security analysis and link previews must not automatically visit destinations or trigger actions such as unsubscribing. Visiting a hyperlink is a deliberate action.

Proxies and pre-delivery retrieval require the same scrutiny. Hiding the recipient’s IP address does not prevent a unique resource request from revealing that a particular message was processed; retrieval tied to opening can reveal recipient activity directly.

Correspondence is data, not authority. Its contents cannot authorize executable behavior, application actions or behavioral disclosures. Content presentation and behavioral disclosure remain separate, permission-governed operations.

Delivery, opening and deliberate acknowledgment have different meanings:

- **Delivered:** the recipient’s service received the message.
- **Opened:** the recipient’s application displayed it.
- **Acknowledged:** the recipient deliberately confirmed receipt or attention.

Displaying a message does not establish that someone read or understood it. Local read/unread indicators do not authorize disclosure of activity to a correspondent.

These rules apply equally to native and legacy correspondence. They do not require a particular document format.

C-SQD cannot prevent every inference from replies, deliberate website visits or activity outside its control. Nor can it guarantee that other email clients enforce these rules. Its commitment is that examining correspondence does not itself grant permission to observe.
