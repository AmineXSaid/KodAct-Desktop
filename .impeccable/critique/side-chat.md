# Side question card (`/btw`) and the model icon

Reviewed on Linux screenshots (Xvfb, lavapipe), light and dark, demo mode.

## Fixed

- The field's text sat 21px in from the field's edge (the field's own padding
  on top of the input's); the field was 8px in while the thread was 14px in.
  Now one padding, the field 10px in, `field_h` tall, its edge darkening with
  focus as the composer's does.
- Send was filled while there was nothing to send. Filled only with text.
- Question and answer differed by 1px of size and a grey. The question is now
  a bubble to the right, as the chat sets yours.
- No way to read a long answer: a Larger / Smaller button grows the card to
  560 wide with a thread up to 560 tall, never past the window.
- The model's icon was Lucide's `cpu`, the stock "AI chip". `model.svg` is our
  own: a facet inside a facet, in the mark's 45° angles, on Lucide's grid and
  stroke. `cpu` stays for a tool of no known kind.

## Left

- P2: grown, the card can cover a permission prompt's second button in the
  chat. Minimise or Smaller uncovers it; the card could move aside instead.
- P3: Clear sits beside Larger; a mis-click clears the thread without undo.
