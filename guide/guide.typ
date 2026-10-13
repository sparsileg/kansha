// Kansha User's Guide: pandoc template producing Typst.
// pandoc fills in the version and the chapters; build-guide.sh runs it.

#let version = "$version$"
#let accent = rgb("#2b4f6e")
#let ink = rgb("#1d232b")
#let muted = rgb("#5b6472")
#let rule-color = rgb("#d5dae1")
#let code-fill = rgb("#f3f5f8")
#let sans = "IBM Plex Sans"

// pandoc writes Markdown's --- as this. The chapters use --- between
// sections, where the headings already separate them, so leave a gap only
#let divider() = v(0.3em)

// Callout boxes, written in the chapters as ::: tip, ::: why, or ::: note
// (callouts.lua turns them into calls to this)
#let callout-styles = (
  tip: (title: "Kansha Tip", color: rgb("#b7791f"), fill: rgb("#fdf6e7")),
  why: (title: "Why this matters", color: accent, fill: rgb("#eef3fa")),
  note: (title: "Note", color: muted, fill: rgb("#f3f5f8")),
  shot: (title: "Screenshot", color: rgb("#9aa3ae"), fill: white),
)
#let callout(kind, title: none, body) = {
  let style = callout-styles.at(kind)
  block(
    width: 100%,
    fill: style.fill,
    stroke: if kind == "shot" { (paint: style.color, thickness: 1pt, dash: "dashed") } else { (left: 3pt + style.color) },
    inset: (left: 12pt, right: 10pt, y: 9pt),
    above: 1.3em,
    below: 1.3em,
    breakable: false,
  )[
    #block(below: 0.65em, text(font: sans, size: 9pt, weight: "semibold", fill: style.color)[#(if title == none { style.title } else { title })])
    #set par(spacing: 0.8em)
    #body
  ]
}

// A captured screenshot, from ::: shot when images/<id>.png exists
// (callouts.lua); the caption is the text written inside the shot box.
// The height cap keeps tall window captures from filling the page.
#let screenshot(path, caption) = figure(
  block(stroke: 0.6pt + rule-color, image(path, width: 100%, height: 14cm, fit: "contain")),
  caption: caption,
)
#show figure.where(kind: image): set figure.caption(position: bottom)
#show figure.where(kind: image): set block(above: 1.3em, below: 1.3em)
#show figure.caption: set text(font: sans, size: 9pt, fill: muted)

#set document(title: "Kansha User's Guide", author: "Kansha")
#set text(font: "IBM Plex Serif", size: 10.5pt, fill: ink, lang: "en")
#set par(justify: true, leading: 0.7em, spacing: 1.15em)
#show raw: set text(font: "IBM Plex Mono", size: 0.88em)
#show raw.where(block: true): block.with(fill: code-fill, inset: 9pt, radius: 3pt, width: 100%)
#show link: set text(fill: accent)
#set list(indent: 0.5em, spacing: 0.75em)
#set enum(indent: 0.5em, spacing: 0.75em)

#set table(
  inset: (x: 7pt, y: 5pt),
  stroke: (_, y) => if y == 0 { (bottom: 0.8pt + accent) } else { (bottom: 0.4pt + rule-color) },
)
#show table: set text(size: 9.5pt)
// pandoc wraps each table in a figure, which won't break across pages
#show figure.where(kind: table): set block(breakable: true)
#show table: set par(justify: false)
#show table.cell.where(y: 0): set text(font: sans, weight: "semibold")

// Chapters are numbered; sections below them are not. Appendices are
// lettered (appendices.lua switches over at the first one)
#show heading.where(level: 1, outlined: true): set heading(numbering: "1", supplement: [Chapter])
#show heading: set text(font: sans, fill: accent, weight: "semibold")
#show heading: set par(justify: false)
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  v(1.8cm)
  if it.numbering != none {
    text(size: 11pt, fill: muted, weight: "regular")[#it.supplement #counter(heading).display()]
  }
  v(0.1cm)
  text(size: 24pt, it.body)
  v(0.15cm)
  line(length: 100%, stroke: 1pt + accent)
  v(0.5cm)
}
#show heading.where(level: 2): set text(size: 15pt)
#show heading.where(level: 2): set block(above: 1.8em, below: 0.9em)
#show heading.where(level: 3): set text(size: 12pt)
#show heading.where(level: 3): set block(above: 1.5em, below: 0.8em)
#show heading.where(level: 4): set text(size: 10.5pt, fill: ink)
#show heading.where(level: 4): set block(above: 1.3em, below: 0.7em)

// Title page
#page(margin: (x: 2.5cm, y: 3cm))[
  #v(1fr)
  #align(center)[
    #image("/kansha-mark.png", height: 7cm)
    #v(0.9cm)
    #text(font: sans, size: 32pt, weight: "semibold", fill: accent)[Kansha]
    #v(0.1cm)
    #text(font: sans, size: 16pt, fill: muted)[User's Guide]
    #v(1.2cm)
    #text(font: sans, size: 10.5pt, fill: muted)[
      Version #version #h(0.4em) · #h(0.4em) #datetime.today().display("[month repr:long] [year]")
    ]
  ]
  #v(1.5fr)
]

#set page(
  paper: "us-letter",
  margin: (x: 2.3cm, top: 2.6cm, bottom: 2.4cm),
  header: context {
    let here-page = here().page()
    let opens-chapter = query(heading.where(level: 1)).any(h => h.location().page() == here-page)
    let chapters = query(heading.where(level: 1).before(here()))
    if chapters.len() > 0 and not opens-chapter {
      set text(font: sans, size: 8.5pt, fill: muted)
      grid(columns: (1fr, auto), chapters.last().body, [Kansha User's Guide · #version])
      v(-0.4em)
      line(length: 100%, stroke: 0.4pt + rule-color)
    }
  },
  footer: context align(center, text(font: sans, size: 8.5pt, fill: muted, counter(page).display())),
)
#counter(page).update(1)

// Contents
#show outline.entry.where(level: 1): it => {
  v(0.9em, weak: true)
  text(font: sans, weight: "semibold", it)
}
#outline(title: [Contents], depth: 2, indent: 1.2em)

$body$
