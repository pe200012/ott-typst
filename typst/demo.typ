#import "ott.typ": ott, render

= Ott demo (rendered by ott-wasm)

== Inline snippet

#ott[
```ott
metavar ident , x  ::= {{ lex alphanum }}

grammar

 expr , e  :: Expr_ ::=
  |  x  :: :: var
  |  () :: :: unit
```
]

== TAPL Arrow (from file)

#render(read("../fixtures/tapl/arrow.ott"))
