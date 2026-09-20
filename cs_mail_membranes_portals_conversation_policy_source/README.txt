Membranes and Portals: A Spatial Interface for cs-mail
Conversation-level privacy and reuse revision, September 2026

FILES
  cs_mail_membranes_portals_memo_conversation_policy.tex
  a_wide_high_resolution_ui_concept_collage_with_a.png
  a_polished_futuristic_ui_concept_illustration_ci.png

BUILD
Keep both PNG files in the same directory as the .tex file. With a standard
TeX Live or MiKTeX installation, run from that directory:

  pdflatex -interaction=nonstopmode -halt-on-error cs_mail_membranes_portals_memo_conversation_policy.tex
  pdflatex -interaction=nonstopmode -halt-on-error cs_mail_membranes_portals_memo_conversation_policy.tex

The second pass resolves cross-references. This revision was compiled and
visually checked as a 19-page PDF. The icons and reference-lifecycle diagram
are drawn in LaTeX; no external font or icon asset is included or needed.
The source uses common LaTeX packages, including TikZ, tabularx and accsupp.

REVISION
Section 7 defines one current conversation-level reuse policy, common to
Reference, Quotation and Sharing and applicable to all conversation messages.
It defaults to reuse within and across relationships, with a within-originating-
relationship alternative. There are no per-message or per-operation policies.
Section 6.1 uses Reference (U+2196) and Share (U+21F1) notation and the agreed
operation definitions. Composition, legacy transport, worked examples, privacy,
implementation and appendix notes are reconciled with the common policy.
Both historical mockups are preserved without alteration.

Open implementation matters are identified in Section 7.5 rather than silently
resolved through new message-level policies or automatic retraction guarantees.
