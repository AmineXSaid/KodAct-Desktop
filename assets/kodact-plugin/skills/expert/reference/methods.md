# Teaching methods, and when each helps

Each method below has: what it is, when to use it, and a short example in the
setting of software work.

## Worked example, then faded steps

**What:** solve one problem fully, narrating each decision. Then give a second
problem of the same kind with the first steps done and the last ones left to the
learner. Then a third with fewer steps given.

**When:** anything procedural: a git workflow, a refactoring, writing a test,
reading a profile.

**Example:** writing a unit test.
1. Fully worked: the test for `parse_price("12.50 €")`, with each choice said
   aloud (arrange, act, assert; why this input; why `assert_eq`).
2. Faded: the test for `parse_price("abc")`, with the arrange step written and
   "what should the assertion be, and why?" left open.
3. Independent: "Write the test for a negative price."

## Retrieval practice

**What:** the learner recalls from memory instead of re-reading. Recalling
strengthens memory far more than seeing the material again.

**When:** at the end of every explanation, and at the start of a later one.

**Good questions:** predict an output; find the bug in five lines; pick the
right tool for a situation and say why; explain a term in one sentence.
**Weak questions:** "Does that make sense?" (it asks for a feeling, not recall).

## Elaborative interrogation ("why?")

**What:** after a fact, ask *why* it holds, and connect the answer to something
the learner already knows.

**Example:** "Indexes speed up reads. *Why* do they slow down writes?" Because
every insert must also update each index, like keeping a book's index current
whenever a page changes.

## Concrete before abstract

**What:** an instance first, then the general rule it shows.

**Example:** show two requests racing on the same counter and the wrong total
they produce, *then* introduce "race condition" and "atomic operation".

## Misconceptions, named

**What:** state the common wrong model and the single fact that breaks it.
Learners rarely drop a wrong model they haven't been shown is wrong.

**Example:** "Many think `async` makes code run in parallel. It doesn't: on one
thread, `async` lets the code *wait* without blocking. Parallelism needs more
threads or processes."

## Dual coding

**What:** words plus a picture of the structure. Two channels beat one.

**When:** flows (a request's path), structures (a data model), states (a
lifecycle), comparisons. Draw with the `diagram-design` skill; label the one part
the lesson is about.

## Analogy, with its limit

**What:** map the new idea onto a familiar one, then say where the map breaks.

**Example:** "A Git branch is like a bookmark on a commit, not a copy of the
files. Where it breaks: a bookmark doesn't move by itself, while a branch moves
forward with every commit you make on it."

## Interleaving

**What:** in practice sets, mix problem types instead of repeating one. It
trains choosing *which* method to use, not just applying one.

**Example:** after teaching `map`, `filter` and `reduce`, give five mixed
problems where the learner must pick which one fits.

## Spaced recap

**What:** come back to an idea after some time, briefly, before building on it.

**Example:** three answers later: "Quick recall: what does an index cost on
writes? (One line.) Good, because the next step depends on it."

## Socratic guidance

**What:** lead with questions; give hints in rising strength.

**Hint ladder:** (1) a question that points at the right area; (2) the concept
to apply; (3) the first step of the solution; (4) the answer, only if asked or
after three hints.

## Feedback that teaches

**What:** for each answer, say what is right, name the exact misunderstanding
behind what is wrong, and give one next step.

**Example:** "You're right that the loop runs 10 times. The output is 9, not
10, because `range(10)` starts at 0, so the last value is 9. Try: what does
`range(2, 10, 3)` give?"
