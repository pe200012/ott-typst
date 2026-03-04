#import "ott.typ": render, ott-file

= TAPL Arrow (spec + term parsing demo)

== Render the spec

#render(read("../fixtures/tapl/arrow.ott"))

== Parse and typeset terms

#let ott = ott-file(read("../fixtures/tapl/arrow.ott"), root: "t")

- var: #ott[x]
- abs (no spaces): #ott[`\x.x`]
- abs (spaced): #ott[`\ x . x`]
- subst (no spaces): #ott[`[x|->x]x`]

In math:

$ #ott[`\x.x`] --> #ott[x] $
