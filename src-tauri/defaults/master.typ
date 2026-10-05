// MASTER CV — replace this sample via Settings → Master CV → "Import my CV",
// or edit it directly. Every tailored CV is derived from this file.
#set page(paper: "a4", margin: (x: 1.8cm, y: 1.6cm))
#set text(font: ("Liberation Sans", "DejaVu Sans", "Libertinus Serif"), size: 10pt, lang: "en")
#set par(justify: true)
#let accent = rgb("#2563eb")
#show heading.where(level: 1): it => block(below: 0.5em, text(size: 22pt, weight: "bold", it.body))
#show heading.where(level: 2): it => block(above: 1.1em, below: 0.6em)[
  #text(fill: accent, size: 11pt, weight: "bold", upper(it.body))
  #v(-0.6em) #line(length: 100%, stroke: 0.5pt + accent)]
#let entry(title, org, dates, body) = [
  *#title* --- #org #h(1fr) #text(fill: gray)[#dates] \
  #body
]

= Jane Doe
Software Engineer · Berlin · jane\@example.com · +49 123 456 789 · linkedin.com/in/janedoe

== Profile
Backend engineer with 5 years of experience building reliable services in *Rust* and *Python*.

== Experience
#entry("Senior Software Engineer", "Acme GmbH, Berlin", "2022 – today")[
  - Built a billing platform processing 2M events/day
  - Led a team of 4 engineers
]
#entry("Software Engineer", "Foo AG, Munich", "2019 – 2022")[
  - Migrated monolith to services, cutting deploy time by 70%
]

== Education
#entry("B.Sc. Computer Science", "TU Berlin", "2015 – 2019")[]

== Skills
#table(columns: 2, stroke: none, inset: (x: 0pt, y: 3pt), column-gutter: 12pt,
  [*Languages*], [Rust, Python, TypeScript, SQL],
  [*Tools*], [Docker, Kubernetes, PostgreSQL, AWS],
  [*Spoken*], [English (C2), German (B2)])
