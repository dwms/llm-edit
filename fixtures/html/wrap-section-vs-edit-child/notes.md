# html/wrap-section-vs-edit-child

## Scenario
A `<section>` with three children: heading, paragraph, button.

## Agent A's intent
Wrap the three children in a new `<div class="container">` for layout purposes.

## Agent B's intent
Rewrite the `<p>` text for clarity.

## Why text-diff merge fails
A adds the wrapper and re-indents all three children by two spaces. B changes one line of text at the original indent. Every child line is simultaneously touched by A (indent shift) and B (only the `<p>`) — diff3 emits a large conflict spanning the whole section body.

## Why a human also struggles
To resolve correctly, the human must:
1. Keep A's wrapper + re-indentation.
2. Transplant B's new `<p>` text into the re-indented position inside the wrapper.

"Take both" produces garbage — duplicated children at two indent levels.

## Why structured op-logs make this trivial
- Agent A's op: `wrap_children(selector=section, wrapper=<div class="container">)`.
- Agent B's op: `set_text(selector=section > p, new="Pick one of the options below.")`.
- Merger: both ops target semantic positions, not line numbers. A's wrap doesn't affect B's selector resolution. Apply both in any order.
