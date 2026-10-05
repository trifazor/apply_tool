# Typst file rules (technical)
- Documents are Typst (.typ) files. They are compiled to PDF with typst and converted to DOCX/ODT with pandoc, so keep the markup simple:
  headings (`=`, `==`), paragraphs, `*bold*`, `_italic_`, lists (`-`), `#table`, `#h(1fr)`, `#v()`, `#line`, `#text(...)`, `#set`/`#show` rules and small `#let` helper functions are fine.
- Do NOT use external packages (`#import "@preview/..."`), `#place`, absolute positioning, or complex layout code.
- Escape `@` in e-mail addresses as `\@`, and `#`, `$` when meant literally.
- Keep the `#set page(paper: "a4", ...)` line.
