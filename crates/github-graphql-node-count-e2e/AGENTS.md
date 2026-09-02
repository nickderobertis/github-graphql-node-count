# The consumer-shaped suite

This project takes the crate as an ordinary dependency and drives only its
published surface — no private item, no helper reaching inside. That is what it
exists to prove: a dependent meeting the crate for the first time gets the
documented answers.

It carries GitHub's two worked examples, transcribed as the documents GitHub
publishes rather than reconstructed to reach a number, with the totals GitHub's
own arithmetic states. The totals come from GitHub's documentation page as it
stood when they were written down here; a later edit to that page cannot make
correct work fail, so re-read it deliberately rather than as part of a change.

It also carries the shape a real consumer assembles — one shared fragment
concatenated onto each of several operations, giving several single-operation
documents — because that is how `onetaskgraph-github-projects` builds its query
constants, and each assembled document must reach its own distinct total.

`frozen_surface` in `tests/public_api.rs` asserts the three promised items at
compile time, from outside the crate. Renaming, retyping or narrowing any of them
stops that file compiling — which is where the break belongs, rather than in the
repository written against them.
